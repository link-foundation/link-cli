using static Foundation.Data.Doublets.Cli.Engine.PageFormat;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class CheckpointPagedCommitLog
{
    /// <summary>Read owned committed records without identity allocation,
    /// initialization, truncation or repair completion. Devices stay caller-owned.
    /// Checked mirror reads may stage volatile repairs; discard those wrappers
    /// if higher-level semantic validation fails.</summary>
    public static IReadOnlyList<string> Inspect(IPageDevice root, IPageDevice first, IPageDevice second, ulong maxPages = MaxPages)
    {
        ArgumentNullException.ThrowIfNull(root); ArgumentNullException.ThrowIfNull(first); ArgumentNullException.ThrowIfNull(second);
        if (ReferenceEquals(root, first) || ReferenceEquals(root, second) || ReferenceEquals(first, second)) throw new ArgumentException("Checkpoint devices must be distinct exclusive owners.");
        if (maxPages == 0 || maxPages > MaxPages) throw new ArgumentOutOfRangeException(nameof(maxPages));
        uint flags = first.MetadataFlags, rootFlags = root.MetadataFlags;
        if ((rootFlags != 0 && rootFlags != (2 | flags)) || flags > 1 || second.MetadataFlags != flags) throw Invalid("inspection requires matching root and bank modes");
        if (root.DataLength != 0) throw Invalid("checkpoint root data must be empty");
        var decision = CheckpointDecision.Select(new[] { root.ReadMetadata(0), root.ReadMetadata(1) }, rootFlags | flags);
        if (decision is null)
        {
            foreach (var bank in new[] { first, second })
            {
                if (bank.DataLength != 0) throw Invalid("unrooted checkpoint bank contains data");
                for (int slot = 0; slot < 2; slot++) if (Metadata.Decode(bank.ReadMetadata(slot), flags) is { Pages: > 0 })
                        throw Invalid("unrooted checkpoint bank contains committed metadata");
            }
            return Array.Empty<string>();
        }
        var selected = decision.Bank == 0 ? first : second;
        var raw = new[] { selected.ReadMetadata(0), selected.ReadMetadata(1) };
        var valid = raw.Select(bytes => Metadata.Decode(bytes, flags)).OfType<Metadata>().ToArray();
        if (valid.Length == 0) throw Invalid("checkpoint selected bank has no valid metadata");
        if (valid.Any(state => !state.Identity.AsSpan().SequenceEqual(decision.BankIdentity()))) throw Invalid("checkpoint bank epoch identity changed");
        if (valid.Length == 2 && valid[0].Generation == valid[1].Generation && !raw[0]!.AsSpan().SequenceEqual(raw[1])) throw Invalid("conflicting metadata decisions");
        var state = valid.MaxBy(item => item.Generation)!;
        if (state.Pages > maxPages) throw Invalid("committed pages exceed selected limit");
        if (selected.DataLength < checked((long)state.Pages * PageSize)) throw Invalid("truncated committed pages");
        return PagedCommitLog.ReadCommitted(selected, state, retain: true);
    }
}
