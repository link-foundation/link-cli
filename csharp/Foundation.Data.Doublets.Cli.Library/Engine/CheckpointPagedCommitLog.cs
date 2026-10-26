using System.Text;
using static Foundation.Data.Doublets.Cli.Engine.PageFormat;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Optional complete-journal replacement under the same exclusive owner.
/// Unknown outcomes fence until actual reopening. False means identical records.</summary>
public interface IReplaceableCommitLog : ICommitLog
{
    bool SupportsReplacement => true;
    bool Replace(IReadOnlyList<string> records);
}

/// <summary>Two fixed page banks and two alternating root decisions. Devices are
/// distinct, exclusive and caller-owned; this log never closes them. Root data
/// stays empty. Replacement prepares and synchronizes an inactive bank before
/// publishing its epoch-bound root. Failed I/O fences until reopening.</summary>
public sealed class CheckpointPagedCommitLog : IReplaceableCommitLog
{
    private readonly IPageDevice _root;
    private readonly IPageDevice[] _banks;
    private readonly ulong _maxPages;
    private CheckpointDecision _decision;
    private PagedCommitLog _active;
    private bool _pending, _fenced;
    private static readonly UTF8Encoding Utf8 = new(false, true);
    public ulong Epoch => _decision.Epoch;
    public bool SupportsReplacement => true;
    public bool SupportsDurableFlush => _root.SupportsDurableFlush && _banks.All(d => d.SupportsDurableFlush);

    public CheckpointPagedCommitLog(IPageDevice root, IPageDevice first, IPageDevice second, ulong maxPages = MaxPages)
    {
        ArgumentNullException.ThrowIfNull(root); ArgumentNullException.ThrowIfNull(first); ArgumentNullException.ThrowIfNull(second);
        if (ReferenceEquals(root, first) || ReferenceEquals(root, second) || ReferenceEquals(first, second)) throw new ArgumentException("Checkpoint devices must be distinct exclusive owners.");
        if (maxPages == 0 || maxPages > MaxPages) throw new ArgumentOutOfRangeException(nameof(maxPages));
        _root = root; _banks = new[] { first, second }; _maxPages = maxPages;
        uint flags = first.MetadataFlags;
        if (root.MetadataFlags != 0 || flags > 1 || second.MetadataFlags != flags) throw Invalid("checkpoint requires a raw root and matching bank modes");
        if (root.DataLength != 0) throw Invalid("checkpoint root data must be empty");
        var decision = CheckpointDecision.Select(new[] { root.ReadMetadata(0), root.ReadMetadata(1) }, flags);
        if (decision is not null)
        {
            bool found = false;
            for (int slot = 0; slot < 2; slot++)
            {
                if (Metadata.Decode(_banks[decision.Bank].ReadMetadata(slot), flags) is { } state)
                {
                    if (!state.Identity.AsSpan().SequenceEqual(decision.BankIdentity())) throw Invalid("checkpoint bank epoch identity changed");
                    found = true;
                }
            }
            if (!found) throw Invalid("checkpoint selected bank has no valid metadata");
        }
        else
        {
            foreach (var bank in _banks)
            {
                if (bank.DataLength != 0) throw Invalid("unrooted checkpoint bank contains data");
                for (int slot = 0; slot < 2; slot++) if (Metadata.Decode(bank.ReadMetadata(slot), flags) is { Pages: > 0 })
                        throw Invalid("unrooted checkpoint bank contains committed metadata");
            }
            byte[] identity = root.NewDatabaseId(); if (identity.Length != 16) throw Invalid("checkpoint root identity length");
            decision = new CheckpointDecision(identity.ToArray(), 0, flags);
            for (int i = 0; i < 2; i++) Reset(_banks[i], (decision with { Epoch = (ulong)i }).BankIdentity(), flags);
            root.SyncData(); root.WriteMetadata(0, decision.Encode()); root.SyncMetadata(0);
        }
        _decision = decision; _active = new PagedCommitLog(_banks[decision.Bank], maxPages);
    }
    private static void Reset(IPageDevice device, byte[] identity, uint flags)
    {
        device.TruncateData(0); device.SyncData(); byte[] empty = new Metadata(identity, Flags: flags).Encode();
        for (int slot = 0; slot < 2; slot++) { device.WriteMetadata(slot, empty); device.SyncMetadata(slot); }
    }
    private void Check() { if (_fenced) throw Invalid("uncertain checkpoint page owner is fenced; reopen all devices"); }
    public IReadOnlyList<string> Read()
    { Check(); try { return _active.Read(); } catch { _fenced = true; throw; } }
    public void Append(string record) { Check(); _active.Append(record); _pending = true; }
    public void Flush()
    { Check(); try { _active.Flush(); _pending = false; } catch { _fenced = true; throw; } }
    public bool Replace(IReadOnlyList<string> records)
    {
        Check(); ArgumentNullException.ThrowIfNull(records);
        if (_pending) throw Invalid("checkpoint replacement rejects pending physical records");
        var copy = records.ToArray(); ulong rows = 0;
        foreach (string record in copy)
        {
            ArgumentNullException.ThrowIfNull(record);
            if (record.Contains('\r') || record.Contains('\n')) throw new ArgumentException("Checkpoint records cannot contain line breaks.", nameof(records));
            rows = checked(rows + ((ulong)Utf8.GetByteCount(record) + 6) / 7 + 1);
        }
        if (rows / Cells + (rows % Cells == 0 ? 0UL : 1UL) > _maxPages) throw Invalid("checkpoint exceeds selected page limit");
        if (Read().SequenceEqual(copy)) return false;
        if (_decision.Epoch == ulong.MaxValue) throw Invalid("checkpoint epoch exhausted");
        var next = _decision with { Epoch = _decision.Epoch + 1 }; _fenced = true;
        Reset(_banks[next.Bank], next.BankIdentity(), next.Flags);
        var prepared = new PagedCommitLog(_banks[next.Bank], _maxPages);
        foreach (string record in copy) prepared.Append(record);
        prepared.Flush(); _root.WriteMetadata(next.Bank, next.Encode()); _root.SyncMetadata(next.Bank);
        _active = prepared; _decision = next; _fenced = false; return true;
    }
}
