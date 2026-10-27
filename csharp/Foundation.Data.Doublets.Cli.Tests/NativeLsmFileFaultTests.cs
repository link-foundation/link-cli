using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeLsmFileFaultTests
{
    private static FileLsmOptions Options => new(PhysicalLsmHarness.Options(), 128);
    private static void Put(FileLsmJournalStore store, string key, string value) => Assert.True(store.CompareExchange(new[] { key }, store.Read(new[] { key }), new IReadOnlyList<string>?[] { new[] { value } }));
    private static void Seed(string directory)
    { using var store = FileLsmJournalStore.Open(directory, Options); Put(store, "a", "old"); Put(store, "b", "second"); store.Flush(); Put(store, "a", "selected"); store.Flush(); Put(store, "c", "buffer"); }
    private static string[] Values(FileLsmJournalStore store) => store.Read(new[] { "a", "b", "c" }).Select(value => value.Generation + ":" + string.Join("/", value.Entries)).ToArray();
    private static IEnumerable<Exception> Causes(Exception error) => error is AggregateException aggregate ? aggregate.Flatten().InnerExceptions : new[] { error };
    private static ulong[] CreatedIds(IEnumerable<string> trace) => trace.Where(name => System.Text.RegularExpressions.Regex.IsMatch(name, "^create:table-[0-9]+\\.writer$"))
        .Select(name => ulong.Parse(name.AsSpan(13, name.Length - 20), System.Globalization.CultureInfo.InvariantCulture)).ToArray();
    [Theory(Timeout = 120000)]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    [InlineData(7)]
    public Task EveryFiniteCompactionOperationCutPreservesAcknowledgedState(int group) => Task.Run(() =>
    {
        string directory = Directory.CreateTempSubdirectory("lsm-native-cuts-").FullName;
        var io = new NativeLsmFileHarness();
        try
        {
            Seed(directory); var baseline = FileLsmJournalStore.Open(directory, Options, io); io.Reset(); baseline.Compact();
            string[] trace = io.Trace.ToArray(); io.Active = false; baseline.Dispose(); Assert.Empty(io.Live); Assert.InRange(trace.Length, 30, 192);
            foreach (string prefix in new[] { "create:", "write:", "truncate:", "sync:", "close:", "delete:" }) Assert.Contains(trace, entry => entry.StartsWith(prefix, StringComparison.Ordinal));
            if (!OperatingSystem.IsWindows()) Assert.Contains(trace, entry => entry.StartsWith("directory-sync:", StringComparison.Ordinal));
            for (int at = group + 1; at <= trace.Length; at += 8) foreach (bool after in new[] { false, true })
                {
                    foreach (string file in Directory.GetFiles(directory)) File.Delete(file); Seed(directory);
                    string[] expected = Array.Empty<string>(); ulong[] attempted = Array.Empty<ulong>();
                    try
                    {
                        using var owner = new NativeLsmFileHarness.Owner(directory, Options, io); var store = owner.Store;
                        expected = Values(store); io.Reset((at, after));
                        var error = Assert.ThrowsAny<Exception>(store.Compact); Assert.Contains(Causes(error), cause => cause.Message.Contains("native LSM operation cut", StringComparison.Ordinal)); Assert.True(io.Hit);
                        attempted = CreatedIds(io.Trace);
                        int count = io.Trace.Count; Assert.ThrowsAny<Exception>(() => store.Read(new[] { "a" })); Assert.Equal(count, io.Trace.Count); io.Active = false;
                    }
                    finally { io.Recover(directory); }
                    using var reopened = FileLsmJournalStore.Open(directory, Options, io); Assert.Equal(expected, Values(reopened)); io.Reset(); Put(reopened, "d", "following"); reopened.Flush();
                    foreach (ulong next in CreatedIds(io.Trace)) foreach (ulong previous in attempted) Assert.True(next > previous, "Reserved table identities are never reused."); io.Active = false;
                }
            Console.WriteLine($"native C# compaction group {group + 1}/8: {trace.Length} operation boundaries, both sides, finite cuts");
        }
        finally { io.Recover(directory); Directory.Delete(directory, true); }
    });
}
