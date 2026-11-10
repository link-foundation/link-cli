using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Names one position in the write set of the session that created it.</summary>
public readonly record struct EngineSavepoint(ulong Id);

internal static class EngineSavepoints
{
    private static ulong s_last;
    // Process-wide, so a token from another session can never select a savepoint.
    public static ulong Next() => Interlocked.Increment(ref s_last);
}

// A savepoint copies the private write set, so it costs O(staged writes) when
// taken and nothing for a session that never takes one. Rolling back restores
// rows only. Observed reads stay validated, because later work may depend on
// them; point-write ownership stays until the session ends, and allocated
// addresses stay retired, matching the gaps a complete rollback leaves.
public sealed partial class EngineSession<T>
{
    private int SavepointPosition(EngineSavepoint savepoint)
    {
        CheckOpen();
        var position = _savepoints.FindIndex(item => item.Id == savepoint.Id);
        return position >= 0 ? position : throw new InvalidOperationException("Unknown savepoint.");
    }
    /// <summary>Remember the current write set. Savepoints nest.</summary>
    public EngineSavepoint Savepoint()
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            var id = EngineSavepoints.Next();
            _savepoints.Add((id, _writes.ToArray()));
            return new(id);
        }
    }
    /// <summary>Discard writes made after this savepoint and every later savepoint.
    /// The savepoint itself stays usable, as in PostgreSQL and SQLite.</summary>
    public void RollbackTo(EngineSavepoint savepoint)
    {
        lock (_engine.Gate)
        {
            var position = SavepointPosition(savepoint);
            _savepoints.RemoveRange(position + 1, _savepoints.Count - position - 1);
            _writes.Clear();
            foreach (var (id, row) in _savepoints[position].Writes) _writes[id] = row;
        }
    }
    /// <summary>Forget this savepoint and every later one, keeping all writes.</summary>
    public void ReleaseSavepoint(EngineSavepoint savepoint)
    {
        lock (_engine.Gate)
        {
            var position = SavepointPosition(savepoint);
            _savepoints.RemoveRange(position, _savepoints.Count - position);
        }
    }
}
