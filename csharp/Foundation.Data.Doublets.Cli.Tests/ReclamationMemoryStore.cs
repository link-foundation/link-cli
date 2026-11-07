using Foundation.Data.Doublets.Cli.Engine;
using System.Text;
namespace Foundation.Data.Doublets.Cli.Tests;

internal sealed class ReclamationMemoryStore : IJournalMaintenanceStore
{
    internal readonly SortedDictionary<string, JournalVersion> Values = new(StringComparer.Ordinal);
    internal readonly List<(string Operation, string[] Keys)> Calls = new();
    internal ulong Revision;
    internal IReadOnlyList<JournalVersion>? ReadOverride;
    internal IReadOnlyList<JournalNamespaceEntry>? ScanOverride;
    private readonly Dictionary<(string, bool), Queue<Action>> _hooks = new();
    public bool SupportsDurableFlush { get; set; }
    internal void On(string operation, Action action, bool after = false)
    {
        if (!_hooks.TryGetValue((operation, after), out var queue)) _hooks.Add((operation, after), queue = new());
        queue.Enqueue(action);
    }
    private void Hook(string operation, bool after)
    {
        if (_hooks.TryGetValue((operation, after), out var queue) && queue.Count != 0) queue.Dequeue()();
    }
    private T Run<T>(string operation, IReadOnlyList<string> keys, Func<T> action)
    {
        Calls.Add((operation, keys.ToArray())); Hook(operation, false); var value = action(); Hook(operation, true); return value;
    }
    internal int Count(string operation) => Calls.Count(call => call.Operation == operation);
    private ulong Next() => Revision = checked(Revision + 1);
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => Run("read", keys, () =>
    {
        if (ReadOverride is { } result) { ReadOverride = null; return result; }
        return keys.Select(key => Values.TryGetValue(key, out var value) ? new(value.Generation, value.Entries.ToArray()) : new JournalVersion(0, Array.Empty<string>())).ToArray();
    });
    public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates) => Run("swap", keys, () =>
    {
        Assert.Equal(keys.Count, expected.Count); Assert.Equal(keys.Count, updates.Count);
        if (keys.Where((key, index) => (Values.GetValueOrDefault(key)?.Generation ?? 0) != expected[index].Generation).Any()) return false;
        if (updates.All(item => item is null)) return true;
        var generation = Next();
        for (var index = 0; index < keys.Count; index++) if (updates[index] is { } entries) Values[keys[index]] = new(generation, entries.ToArray());
        return true;
    });
    public IReadOnlyList<JournalNamespaceEntry> ScanNamespace(JournalScanOptions? options = null) => Run("scan", Array.Empty<string>(), () =>
    {
        if (ScanOverride is { } resultOverride) { ScanOverride = null; return resultOverride; }
        options ??= new(); long bytes = 0;
        var result = Values.Select(pair =>
        {
            var entries = options.KeysOnly ? Array.Empty<string>() : pair.Value.Entries.ToArray();
            bytes += Encoding.UTF8.GetByteCount(pair.Key) + 8 + entries.Sum(record => (long)Encoding.UTF8.GetByteCount(record) + 1);
            return new JournalNamespaceEntry(pair.Key, new(pair.Value.Generation, entries));
        }).ToArray();
        if (result.Length > options.MaxKeys || bytes > options.MaxBytes) throw new IOException("Complete fake scan limit.");
        return result;
    });
    public bool CompareDelete(string key, ulong expectedGeneration) => Run("delete", new[] { key }, () =>
    {
        Assert.NotEqual(0UL, expectedGeneration);
        if (!Values.TryGetValue(key, out var value) || value.Generation != expectedGeneration) return false;
        Next(); Values.Remove(key); return true;
    });
}
