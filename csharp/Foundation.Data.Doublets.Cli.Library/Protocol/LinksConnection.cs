using DoubletLink = Platform.Data.Doublets.Link<uint>;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// One connection of a <see cref="LinksServer"/>: its subscriptions and the documents waiting to be written to it.
/// </summary>
/// <remarks>
/// <para>
/// Replies and pushes share one outbox, written in order by <see cref="WriteAll"/> on a thread of its own, so a push
/// never interleaves with a reply and a slow subscriber never holds up the store. The server enqueues a reply under
/// its store lock, like the pushes of every write, so a connection reads its replies and pushes in the order the
/// store changed.
/// </para>
/// <para>
/// The subscriptions are read and changed under the server's store lock only.
/// </para>
/// </remarks>
internal sealed class LinksConnection
{
    private readonly object _lock = new();
    private readonly Queue<(IReadOnlyList<LinoLink> Document, MessageFormat Format, bool IsReply)> _outbox = new();
    private readonly SortedDictionary<uint, (LinksRestriction Restriction, MessageFormat Format)> _subscriptions = new();
    private readonly IDisposable _connection;
    private readonly int _maxPendingNotifications;
    private uint _nextSubscription = 1;
    private int _pendingReplies;
    private int _pendingNotifications;
    private bool _closed;

    /// <param name="connection">Disposed when the session closes, which ends both of its threads.</param>
    /// <param name="maxPendingNotifications">Pushes the session holds before it gives up on a reader too slow to take them.</param>
    public LinksConnection(IDisposable connection, int maxPendingNotifications)
    {
        _connection = connection;
        _maxPendingNotifications = maxPendingNotifications;
    }

    /// <summary>The numbers of the subscriptions of this connection, in order.</summary>
    public IReadOnlyCollection<uint> Subscriptions => _subscriptions.Keys;

    /// <summary>Whether the session has been closed.</summary>
    public bool IsClosed
    {
        get
        {
            lock (_lock)
            {
                return _closed;
            }
        }
    }

    /// <summary>Adds a subscription and returns its number; numbers start at 1 on every connection.</summary>
    public uint Subscribe(LinksRestriction restriction, MessageFormat format)
    {
        var subscription = _nextSubscription++;
        _subscriptions[subscription] = (restriction, format);
        return subscription;
    }

    /// <summary>Removes a subscription; an unknown number is ignored.</summary>
    public void Unsubscribe(uint subscription) => _subscriptions.Remove(subscription);

    /// <summary>
    /// Pushes <paramref name="changes"/> to every subscription they fit, one <c>(notify: N)</c> document each; a side
    /// that does not fit (or that <paramref name="readable"/> refuses) is left out, and a change left with no side is
    /// not pushed. Closes the session when too many pushes are already waiting.
    /// </summary>
    public void Notify(IReadOnlyList<(DoubletLink Before, DoubletLink After)> changes, Func<DoubletLink, bool>? readable)
    {
        foreach (var (subscription, (restriction, format)) in _subscriptions)
        {
            bool Fits(DoubletLink link) => !link.IsNull() && restriction.Matches(link) && (readable?.Invoke(link) ?? true);
            var seen = changes
                .Select(change => (Before: Fits(change.Before) ? change.Before : default, After: Fits(change.After) ? change.After : default))
                .Where(change => !change.Before.IsNull() || !change.After.IsNull())
                .ToList();
            if (seen.Count > 0 && !Push(LinksOperation.NotificationDocument(subscription, seen), format))
            {
                return;
            }
        }
    }

    /// <summary>
    /// <see cref="Notify"/> on a server in triplet mode: every subscription is matched part by part against the
    /// triplets, and <paramref name="readable"/> judges the outer doublet of each, as <c>(index: type index)</c>.
    /// </summary>
    public void NotifyTriplets(IReadOnlyList<TripletChange<uint>> changes, Func<DoubletLink, bool>? readable)
    {
        if (changes.Count == 0)
        {
            return;
        }
        foreach (var (subscription, (restriction, format)) in _subscriptions)
        {
            Triplet<uint>? Seen(Triplet<uint>? side) =>
                side is { } triplet
                && TripletsOperation.Matches(triplet, restriction)
                && (readable?.Invoke(new DoubletLink(triplet.Index, triplet.Type, triplet.Index)) ?? true)
                    ? side
                    : null;
            var seen = changes
                .Select(change => new TripletChange<uint>(Seen(change.Before), Seen(change.After)))
                .Where(change => change.Before is not null || change.After is not null)
                .ToList();
            if (seen.Count > 0 && !Push(TripletsOperation.Pack(TripletsOperation.NotificationDocument(subscription, seen), format), format))
            {
                return;
            }
        }
    }

    /// <summary>Queues a reply; <see cref="WaitForReplies"/> waits until it is written.</summary>
    public void Reply(IReadOnlyList<LinoLink> document, MessageFormat format)
    {
        lock (_lock)
        {
            if (_closed)
            {
                return;
            }
            _outbox.Enqueue((document, format, true));
            _pendingReplies++;
            Monitor.PulseAll(_lock);
        }
    }

    /// <summary>
    /// Waits until every queued reply is written, so a client that never reads its replies stops being read from,
    /// as before the server had pushes.
    /// </summary>
    public void WaitForReplies()
    {
        lock (_lock)
        {
            while (_pendingReplies > 0 && !_closed)
            {
                Monitor.Wait(_lock);
            }
        }
    }

    /// <summary>
    /// Writes the outbox with <paramref name="write"/>, in order, until the session closes; returns the error that
    /// broke the connection, if one did.
    /// </summary>
    public Exception? WriteAll(Action<IReadOnlyList<LinoLink>, MessageFormat> write)
    {
        try
        {
            while (true)
            {
                (IReadOnlyList<LinoLink> Document, MessageFormat Format, bool IsReply) next;
                lock (_lock)
                {
                    while (_outbox.Count == 0 && !_closed)
                    {
                        Monitor.Wait(_lock);
                    }
                    if (_closed)
                    {
                        return null;
                    }
                    next = _outbox.Peek();
                }
                write(next.Document, next.Format);
                lock (_lock)
                {
                    _outbox.Dequeue();
                    if (next.IsReply)
                    {
                        _pendingReplies--;
                    }
                    else
                    {
                        _pendingNotifications--;
                    }
                    Monitor.PulseAll(_lock);
                }
            }
        }
        catch (Exception error) when (error is not OutOfMemoryException)
        {
            // The connection broke: its reader stops too.
            Close();
            return error;
        }
    }

    /// <summary>Stops the session and closes its connection; called again, it does nothing.</summary>
    public void Close()
    {
        lock (_lock)
        {
            if (_closed)
            {
                return;
            }
            _closed = true;
            _outbox.Clear();
            Monitor.PulseAll(_lock);
        }
        _connection.Dispose();
    }

    private bool Push(IReadOnlyList<LinoLink> document, MessageFormat format)
    {
        lock (_lock)
        {
            if (_closed)
            {
                return false;
            }
            if (_pendingNotifications < _maxPendingNotifications)
            {
                _outbox.Enqueue((document, format, false));
                _pendingNotifications++;
                Monitor.PulseAll(_lock);
                return true;
            }
        }
        // A subscriber that does not read its pushes would make the server hold every change for it.
        Close();
        return false;
    }
}
