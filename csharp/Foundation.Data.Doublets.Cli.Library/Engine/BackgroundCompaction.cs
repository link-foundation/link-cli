using System.Numerics;
using System.Runtime.ExceptionServices;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>An optional worker that calls <c>CompactJournalAfter</c> every period on
/// its own thread. Each pass that compacts holds the commit lock while the journal
/// is replaced, so commits wait for it. The first failure ends the worker and
/// <see cref="Stop"/> throws it; a failed replacement also fences the engine until
/// reopen, as with a manual compaction. <see cref="Dispose"/> stops the worker and
/// discards any failure.</summary>
public sealed class BackgroundCompaction<T> : IDisposable
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly ManualResetEventSlim _stop = new();
    private readonly Thread _worker;
    private ExceptionDispatchInfo? _failure;
    private bool _disposed;

    /// <summary>Refuses a zero count or period and a log that cannot be replaced
    /// before starting a thread.</summary>
    public BackgroundCompaction(LinksEngine<T> engine, ulong commits, TimeSpan period)
    {
        ArgumentNullException.ThrowIfNull(engine);
        ArgumentOutOfRangeException.ThrowIfZero(commits);
        ArgumentOutOfRangeException.ThrowIfLessThanOrEqual(period, TimeSpan.Zero);
        // Nothing is due yet at this distance; this only checks the selection.
        engine.CompactJournalAfter(ulong.MaxValue);
        _worker = new Thread(() =>
        {
            try
            {
                while (!_stop.Wait(period)) engine.CompactJournalAfter(commits);
            }
            catch (Exception failure)
            {
                _failure = ExceptionDispatchInfo.Capture(failure);
            }
        })
        {
            IsBackground = true,
            Name = "links-engine-compaction",
        };
        _worker.Start();
    }

    /// <summary>Waits for a running pass, then throws the failure that ended the
    /// worker.</summary>
    public void Stop()
    {
        Dispose();
        _failure?.Throw();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _stop.Set();
        _worker.Join();
        _stop.Dispose();
    }
}
