using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

public enum GroupCommitStatus { Committed, Aborted, Indeterminate }
/// <summary>An independent session's outcome in input order. Indeterminate
/// requires reopening to discover the persisted prefix; Revision is absent.</summary>
public sealed record GroupCommitOutcome(GroupCommitStatus Status, ulong? Revision = null, string? Error = null);

public sealed partial class LinksEngine<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    /// <summary>Validate independent sessions in input order, append separate
    /// revisions and flush once before publication. This is not an atomic batch.
    /// Unknown I/O outcomes fence this engine and preserve its old visible state.
    /// Duplicate, null or foreign members fail before consuming any session.
    /// Preparation retains metadata and selected logical revision records;
    /// ordinary commits do not acquire this additional state.</summary>
    public IReadOnlyList<GroupCommitOutcome> CommitGroup(IEnumerable<EngineSession<T>> sessions)
    {
        ArgumentNullException.ThrowIfNull(sessions);
        var members = sessions.ToArray();
        if (members.Any(session => session is null || !ReferenceEquals(session.Owner, this))
            || members.Distinct(ReferenceEqualityComparer.Instance).Count() != members.Length)
            throw new ArgumentException("Group members must be distinct sessions of this engine.", nameof(sessions));
        lock (Gate)
        {
            try { CheckReady(); }
            catch { foreach (var session in members) session.Rollback(); throw; }
            var memory = new MemoryCommitLog();
            var working = new LinksEngine<T>(memory, Options with { Trace = null, RequireDurableFlush = false })
            {
                Current = Current,
                Next = Next,
                CommittedNext = CommittedNext,
                Active = Active,
                Changed = Changed is null ? null : new(Changed),
                WriteOwners = WriteOwners is null ? null : new(WriteOwners)
            };
            working._history.Clear();
            foreach (var (revision, snapshot) in _history) working._history.Add(revision, snapshot);
            var outcomes = new GroupCommitOutcome[members.Length];
            var acceptedSnapshots = new EngineSnapshot<T>?[members.Length];
            for (int i = 0; i < members.Length; i++)
            {
                var session = members[i];
                var original = session.BaseSnapshot;
                try
                {
                    outcomes[i] = new(GroupCommitStatus.Committed, session.CommitLocked(working));
                    acceptedSnapshots[i] = session.BaseSnapshot;
                }
                catch (Exception error) { outcomes[i] = new(GroupCommitStatus.Aborted, Error: error.Message); }
                finally { session.BaseSnapshot = original; }
            }
            // Admission and point ownership end even if the log outcome is
            // unknown. Keep the old Current for other read-committed sessions.
            Active = working.Active;
            WriteOwners = working.WriteOwners;
            var records = memory.Read();
            try
            {
                foreach (var record in records) _log.Append(record);
                if (records.Count > 0) _log.Flush();
            }
            catch (Exception error)
            {
                Poisoned = true;
                if (Active == 0) Changed = null;
                for (int i = 0; i < outcomes.Length; i++)
                    if (outcomes[i].Status == GroupCommitStatus.Committed)
                        outcomes[i] = new(GroupCommitStatus.Indeterminate, Error: $"Indeterminate group commit: {error.Message}; reopen the engine.");
                return outcomes;
            }
            Current = working.Current;
            CommittedNext = working.CommittedNext;
            Changed = working.Changed;
            _history.Clear();
            foreach (var (revision, snapshot) in working._history) _history.Add(revision, snapshot);
            for (int i = 0; i < members.Length; i++)
                if (acceptedSnapshots[i] is { } snapshot) members[i].BaseSnapshot = snapshot;
            if (records.Count > 0) EngineDiagnostics.Trace(Options.Trace, $"Group committed revision={Current.Revision} records={records.Count}");
            return outcomes;
        }
    }
}
