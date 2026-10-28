using System.Numerics;
using System.Text.RegularExpressions;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record JournalVersion(ulong Generation, IReadOnlyList<string> Entries);

/// <summary>Read all keys at one linearizable point; compare every selected key
/// and publish all updates atomically. Generations must never be reused.</summary>
public interface IAtomicJournalStore
{
    bool SupportsDurableFlush { get; }
    IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys);
    bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates);
}

/// <summary>Optional optimistic coordination. Each Begin reads fresh shared
/// journals; local engines are private workspaces. The store is caller-owned.</summary>
public sealed class ClusterEngine<T> where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly IAtomicJournalStore _store;
    private readonly EngineOptions _options;
    private readonly ulong? _journalCheckpointInterval;
    public ClusterEngine(IAtomicJournalStore store, EngineOptions? options = null, ulong? journalCheckpointInterval = null)
    {
        _store = store; options ??= new();
        if (options.Concurrency != ConcurrencyControl.Optimistic || options.Isolation != IsolationLevel.Serializable)
            throw new ArgumentException("Network sessions require optimistic admission and serializable validation.", nameof(options));
        if (options.RequireDurableFlush && !store.SupportsDurableFlush) throw new ArgumentException("Coordinator does not support durable flush.", nameof(store));
        if (journalCheckpointInterval == 0) throw new ArgumentOutOfRangeException(nameof(journalCheckpointInterval));
        _journalCheckpointInterval = journalCheckpointInterval;
        _options = options with { RequireDurableFlush = false };
    }
    public ClusterTransaction<T> Begin(params string[] keys)
    {
        ValidateKeys(keys);
        var expected = _store.Read(keys);
        if (expected.Count != keys.Length) throw new InvalidDataException("Incomplete coordinator snapshot.");
        return new(_store, _options, keys.ToArray(), expected.Select(item => new JournalVersion(item.Generation, item.Entries.ToArray())).ToArray(), _journalCheckpointInterval);
    }
    internal static void ValidateKeys(IReadOnlyList<string> keys)
    {
        if (keys.Count is < 1 or > 32 || keys.Distinct(StringComparer.Ordinal).Count() != keys.Count ||
            keys.Any(key => key is null || !Regex.IsMatch(key, "\\A[A-Za-z0-9_.-]{1,128}\\z", RegexOptions.CultureInvariant)))
            throw new ArgumentException("Select 1..32 distinct shard names using letters, numbers, dot, underscore or dash.", nameof(keys));
    }
}

/// <summary>Single-threaded private group. Commit is the only publication point.</summary>
public sealed class ClusterTransaction<T> : IDisposable where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly IAtomicJournalStore _store;
    private readonly string[] _keys;
    private readonly IReadOnlyList<JournalVersion> _expected;
    private readonly List<(MemoryCommitLog Log, LinksEngine<T> Engine, EngineSession<T> Session)> _work = new();
    private readonly List<ClusterShardSession<T>> _shards = new();
    private bool _closed;
    private readonly ulong? _journalCheckpointInterval;
    internal ClusterTransaction(IAtomicJournalStore store, EngineOptions options, string[] keys, IReadOnlyList<JournalVersion> expected, ulong? journalCheckpointInterval)
    {
        _store = store; _keys = keys; _expected = expected;
        _journalCheckpointInterval = journalCheckpointInterval;
        try
        {
            foreach (var item in expected)
            {
                var log = new MemoryCommitLog(); foreach (var entry in item.Entries) log.Append(entry);
                var engine = new LinksEngine<T>(log, options); var session = engine.Begin();
                _work.Add((log, engine, session)); _shards.Add(new(this, session));
            }
        }
        catch { Dispose(); throw; }
    }
    internal void CheckOpen() { if (_closed) throw new InvalidOperationException("Cluster transaction is closed."); }
    public ClusterShardSession<T> Shard(string key)
    {
        CheckOpen(); var index = Array.IndexOf(_keys, key);
        return index < 0 ? throw new ArgumentException("Shard was not selected at begin.", nameof(key)) : _shards[index];
    }
    public void Commit()
    {
        CheckOpen(); _closed = true;
        try
        {
            var updates = new List<IReadOnlyList<string>?>();
            for (var i = 0; i < _work.Count; i++)
            {
                _work[i].Session.Commit(); var entries = _work[i].Log.Read();
                // Test before compaction: a checkpoint may keep the same length.
                if (entries.Count == _expected[i].Entries.Count) updates.Add(null);
                else updates.Add(_journalCheckpointInterval is { } interval && _work[i].Engine.Snapshot().Revision % interval == 0
                    ? _work[i].Engine.ExportCheckpoint() : entries);
            }
            bool committed;
            try { committed = _store.CompareExchange(_keys, _expected, updates); }
            catch (Exception cause) { throw new IOException("Indeterminate coordinated commit; read fresh journals to discover the outcome.", cause); }
            if (!committed) throw new InvalidOperationException("Conflict: a selected shard changed since begin.");
        }
        finally { foreach (var item in _work) item.Session.Dispose(); }
    }
    public void Rollback() { _closed = true; foreach (var item in _work) item.Session.Rollback(); }
    public void Dispose() => Rollback();
}

/// <summary>Same links/names API; commit the containing group explicitly.</summary>
public sealed class ClusterShardSession<T> : INamedTypesLinks<T> where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly ClusterTransaction<T> _group;
    private readonly EngineSession<T> _session;
    internal ClusterShardSession(ClusterTransaction<T> group, EngineSession<T> session) { _group = group; _session = session; }
    private R Use<R>(Func<EngineSession<T>, R> action) { _group.CheckOpen(); return action(_session); }
    public LinksConstants<T> Constants => _session.Constants;
    public T CreateLink(T source, T target) => Use(s => s.CreateLink(source, target));
    public T EnsureCreated(T id) => Use(s => s.EnsureCreated(id));
    public Link<T>? GetLink(T id) => Use(s => s.GetLink(id));
    public Link<T> UpdateLink(T id, T source, T target) => Use(s => s.UpdateLink(id, source, target));
    public Link<T> DeleteLink(T id) => Use(s => s.DeleteLink(id));
    public string? GetName(T id) => Use(s => s.GetName(id));
    public T SetName(T id, string name) => Use(s => s.SetName(id, name));
    public T GetByName(string name) => Use(s => s.GetByName(name));
    public void RemoveName(T id) { _group.CheckOpen(); _session.RemoveName(id); }
    public T Count(IList<T>? restriction) => Use(s => s.Count(restriction));
    public T Each(IList<T>? restriction, ReadHandler<T>? handler) => Use(s => s.Each(restriction, handler));
    public T Create(IList<T>? substitution, WriteHandler<T>? handler) => Use(s => s.Create(substitution, handler));
    public T Update(IList<T>? restriction, IList<T>? substitution, WriteHandler<T>? handler) => Use(s => s.Update(restriction, substitution, handler));
    public T Delete(IList<T>? restriction, WriteHandler<T>? handler) => Use(s => s.Delete(restriction, handler));
}
