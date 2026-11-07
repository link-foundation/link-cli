using System.Runtime.InteropServices;

namespace Foundation.Data.Doublets.Cli;

/// <summary>Asks a server to stop on Ctrl+C or, as a service manager or container does, on SIGTERM.</summary>
internal sealed class StopSignals : IDisposable
{
    private readonly ConsoleCancelEventHandler _onInterrupt;
    private readonly PosixSignalRegistration _onTerminate;

    public StopSignals(Action stop)
    {
        _onInterrupt = (_, eventArgs) =>
        {
            eventArgs.Cancel = true;
            stop();
        };
        Console.CancelKeyPress += _onInterrupt;
        _onTerminate = PosixSignalRegistration.Create(PosixSignal.SIGTERM, signal =>
        {
            signal.Cancel = true;
            stop();
        });
    }

    public void Dispose()
    {
        Console.CancelKeyPress -= _onInterrupt;
        _onTerminate.Dispose();
    }
}
