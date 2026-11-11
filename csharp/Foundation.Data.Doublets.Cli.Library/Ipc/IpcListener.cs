using System.Collections.Concurrent;
using System.Net;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>Accepts incoming <see cref="IpcStream"/> connections on one endpoint, or several (issue #140).</summary>
public interface IIpcListener : IDisposable
{
    /// <summary>The next connection; null once the listener is closed or can accept no more.</summary>
    IpcStream? Accept();

    /// <summary>The endpoint as clients connect to it, such as <c>shm:/tmp/links</c>.</summary>
    string Endpoint { get; }

    /// <summary>Every endpoint listened on.</summary>
    IReadOnlyList<string> Endpoints => [Endpoint];

    /// <summary>The TCP address, when one of the endpoints is TCP.</summary>
    IPEndPoint? LocalEndPoint => null;

    /// <summary>An action, callable from any thread, that closes the listener and wakes a blocked <see cref="Accept"/>.</summary>
    Action Closer();
}

/// <summary>Listens on several endpoints at once; see <see cref="IpcTransports.ListenAll"/>.</summary>
public sealed class MultiListener : IIpcListener
{
    private readonly BlockingCollection<(IpcStream? Stream, Exception? Error, bool Ended)> _incoming = new();
    private readonly List<Action> _closers;
    private readonly string[] _endpoints;
    private int _open;
    private volatile bool _closed;

    /// <summary>Accepts on every listener; each one runs on a thread of its own.</summary>
    public MultiListener(IReadOnlyList<IIpcListener> listeners)
    {
        ArgumentNullException.ThrowIfNull(listeners);
        _endpoints = [.. listeners.SelectMany(listener => listener.Endpoints)];
        LocalEndPoint = listeners.Select(listener => listener.LocalEndPoint).FirstOrDefault(address => address is not null);
        _closers = [.. listeners.Select(listener => listener.Closer())];
        _open = listeners.Count;
        foreach (var listener in listeners)
        {
            new Thread(() => AcceptAll(listener)) { IsBackground = true, Name = "clink-accept" }.Start();
        }
    }

    private void AcceptAll(IIpcListener listener)
    {
        // The thread owns its listener, as in the Rust implementation, and closes it when it ends.
        using var owned = listener;
        while (true)
        {
            try
            {
                if (listener.Accept() is not { } stream)
                {
                    _incoming.Add((null, null, true));
                    return;
                }
                _incoming.Add((stream, null, false));
            }
            catch (InvalidOperationException) when (_incoming.IsAddingCompleted)
            {
                return;
            }
            catch (Exception error) when (error is IOException or System.Net.Sockets.SocketException or UnauthorizedAccessException)
            {
                try
                {
                    _incoming.Add((null, error, false));
                }
                catch (InvalidOperationException)
                {
                    return;
                }
            }
        }
    }

    /// <inheritdoc/>
    public IpcStream? Accept()
    {
        while (_open > 0 && !_closed)
        {
            var (stream, error, ended) = _incoming.Take();
            if (ended)
            {
                _open--;
                continue;
            }
            if (error is not null)
            {
                throw error is IOException io ? io : new IpcException(IpcErrorKind.Other, error.Message, error);
            }
            return stream;
        }
        return null;
    }

    /// <inheritdoc/>
    public string Endpoint => string.Join(' ', _endpoints);

    /// <inheritdoc/>
    public IReadOnlyList<string> Endpoints => _endpoints;

    /// <inheritdoc/>
    public IPEndPoint? LocalEndPoint { get; }

    /// <inheritdoc/>
    public Action Closer() => () =>
    {
        _closed = true;
        foreach (var closer in _closers)
        {
            closer();
        }
        // Wakes an Accept waiting for the next connection.
        try
        {
            _incoming.Add((null, null, true));
        }
        catch (InvalidOperationException)
        {
        }
    };

    /// <inheritdoc/>
    public void Dispose()
    {
        _closed = true;
        foreach (var closer in _closers)
        {
            closer();
        }
    }
}
