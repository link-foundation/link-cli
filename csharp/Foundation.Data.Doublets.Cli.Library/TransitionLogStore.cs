using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// Append-only store of serialized transitions and recovery markers: the
/// journal of <see cref="TransactionsDecorator{TLinkAddress}"/>.
/// </summary>
/// <remarks>
/// Two implementations ship with the library: the original links-backed
/// journal, where each entry is the name of a fresh link (used by the
/// constructors that take an <see cref="INamedTypesLinks{TLinkAddress}"/>),
/// and <see cref="FileTransitionLog"/>, an append-only text file whose
/// size retention policies bound (issue #195).
/// </remarks>
public interface ITransitionLogStore
{
    /// <summary>Appends one entry. Entries never contain line breaks.</summary>
    void AppendLogEntry(string entry);

    /// <summary>Reads every entry back, oldest first.</summary>
    IReadOnlyList<string> ReadLogEntries();

    /// <summary>Makes every appended entry durable.</summary>
    void FlushLog();

    /// <summary>
    /// Whether <see cref="FlushLog"/> requests persistent storage. This
    /// describes the adapter, not the reliability of its hardware.
    /// </summary>
    bool SupportsDurableFlush => false;

    /// <summary>
    /// Whether <see cref="CompactLogEntries"/> can rewrite the durable
    /// journal. Retention uses it to bound the journal on disk; logs without
    /// it keep every entry, as before.
    /// </summary>
    bool SupportsCompaction => false;

    /// <summary>
    /// Atomically and durably replaces every entry with
    /// <paramref name="entries"/>, the retained journal. After a crash the log
    /// holds either the old or the new journal, never a mixture. Called only
    /// between transactions and only when <see cref="SupportsCompaction"/> is
    /// true.
    /// </summary>
    void CompactLogEntries(IReadOnlyList<string> entries) =>
        throw new NotSupportedException("This transitions log does not support compaction.");
}

/// <summary>
/// The links-backed journal: each entry is the name of a freshly created
/// link in <paramref name="store"/>.
/// </summary>
internal sealed class NamedTypesTransitionLog<TLinkAddress>(INamedTypesLinks<TLinkAddress> store, Action? flush)
    : ITransitionLogStore where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    // A named field, so LinksFacadeDisposer reaches the store behind the journal.
    private readonly INamedTypesLinks<TLinkAddress> _store = store;

    private readonly Action _flush = flush
        ?? (store is IDurableLinks { SupportsDurableFlush: true } durable ? durable.Flush : static () => { });

    public bool SupportsDurableFlush { get; } =
        flush is not null || store is IDurableLinks { SupportsDurableFlush: true };

    public void AppendLogEntry(string entry)
    {
        // Always allocate a fresh link so entries never overwrite one another.
        var link = _store.CreateAndUpdate(_store.Constants.Null, _store.Constants.Null);
        _store.SetName(link, entry);
    }

    public IReadOnlyList<string> ReadLogEntries()
    {
        var entries = new List<string>();
        var any = _store.Constants.Any;
        foreach (var row in _store.All(new Link<TLinkAddress>(any, any, any)))
        {
            if (row is null) continue;
            var name = _store.GetName(new Link<TLinkAddress>(row).Index);
            if (!string.IsNullOrEmpty(name)) entries.Add(name);
        }
        return entries;
    }

    public void FlushLog() => _flush();
}
