
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// <see cref="RemoteLinks"/> as a <see cref="ITriplets{TLinkAddress}"/> store, behind a <see cref="LinksServer"/> in
/// triplet mode (issue #152).
/// </summary>
/// <remarks>
/// Code written against <see cref="ITriplets{TLinkAddress}"/>, <see cref="TripletsQueryProcessor"/> included, switches
/// from a local store to a server by swapping one value. Every call is one <see cref="TripletsOperation"/> round trip.
/// The doublets that store the triplets stay behind the server: over a server in triplet mode, the doublet calls
/// (<c>Create</c>, <c>Update</c>, <c>Delete</c>, <c>Each</c>) are refused like a failed connection; only
/// <c>Exists</c>, which counts, and the names keep working.
/// </remarks>
public sealed partial class RemoteLinks
{
    private TripletMarkers<uint>? _markers;

    /// <summary>Runs one triplet operation on the server and returns its reply.</summary>
    public IReadOnlyList<LinoLink> ExecuteTriplets(TripletsOperation operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        lock (_clientLock)
        {
            return _client.Request(operation.ToDocument());
        }
    }

    /// <summary>Number of triplets matching <paramref name="restriction"/>.</summary>
    public uint CountTripletsMatching(LinksRestriction restriction) =>
        LinksOperation.ParseCount(ExecuteTriplets(new TripletsOperation.Count(restriction)));

    /// <summary>Triplets matching <paramref name="restriction"/>, ordered by index.</summary>
    public IReadOnlyList<Triplet<uint>> TripletsMatching(LinksRestriction restriction) =>
        TripletsOperation.ParseTriplets(ExecuteTriplets(new TripletsOperation.Each(restriction)));

    /// <summary>Runs a <c>create</c>, <c>update</c> or <c>delete</c> and returns its changes.</summary>
    public IReadOnlyList<TripletChange<uint>> TripletChanges(TripletsOperation operation) =>
        TripletsOperation.ParseChanges(ExecuteTriplets(operation));

    /// <summary>
    /// Asks the server to push the changes other connections make to the triplets matching
    /// <paramref name="restriction"/> and returns the number of the subscription, which
    /// <see cref="ReceiveTripletNotification"/> reports them with. <see cref="Unsubscribe"/> stops it.
    /// </summary>
    public uint SubscribeTriplets(LinksRestriction restriction) =>
        LinksOperation.ParseSubscription(ExecuteTriplets(new TripletsOperation.Subscribe(restriction)));

    /// <summary>
    /// The next change pushed to a subscription of this connection, waiting up to <paramref name="timeout"/> for one;
    /// null when none arrived in time.
    /// </summary>
    public TripletsNotification? ReceiveTripletNotification(TimeSpan timeout)
    {
        lock (_clientLock)
        {
            return _client.ReceiveNotification(timeout) is { } document ? TripletsOperation.ParseNotification(document) : null;
        }
    }

    /// <summary>
    /// The markers the server names; one the subject may not read is <c>0</c>. Nothing else of this implementation
    /// depends on them: the server applies the encoding.
    /// </summary>
    public TripletMarkers<uint> Markers
    {
        get
        {
            lock (_clientLock)
            {
                return _markers ??= new TripletMarkers<uint>(
                    GetByName(TripletsOverDoublets<uint>.PairMarkerName),
                    GetByName(TripletsOverDoublets<uint>.NothingMarkerName));
            }
        }
    }

    /// <summary>
    /// A marker, a triplet, or null: the doublets that store the triplets are not on the wire, so they read as missing.
    /// </summary>
    public DoubletRole? RoleOf(uint id)
    {
        if (id == 0)
        {
            return null;
        }
        if (Markers.Contains(id))
        {
            return DoubletRole.Marker;
        }
        return TripletsMatching(LinksRestriction.Of(id)).Count > 0 ? DoubletRole.Triplet : null;
    }

    /// <inheritdoc/>
    public Triplet<uint>? GetTriplet(uint index)
    {
        if (index == 0)
        {
            return null;
        }
        return TripletsMatching(LinksRestriction.Of(index)) is [var triplet] ? triplet : null;
    }

    /// <inheritdoc/>
    public bool IsTriplet(uint index) => GetTriplet(index) is not null;

    /// <inheritdoc/>
    public uint CreateTriplet(uint type, uint from, uint to) => CreatedTriplet(new TripletsOperation.Create(type, from, to));

    /// <inheritdoc/>
    public uint CreateTripletPoint() => CreatedTriplet(new TripletsOperation.Create(0, 0, 0));

    /// <summary>
    /// Makes the free address <paramref name="address"/> a point: <c>create</c> has no way to name an address, a
    /// substitution query does.
    /// </summary>
    public void CreateTripletPointAt(uint address)
    {
        lock (_clientLock)
        {
            _client.Query($"() (({address}: {address} 0 0))");
        }
    }

    /// <inheritdoc/>
    public Triplet<uint> UpdateTriplet(uint index, uint type, uint from, uint to)
    {
        foreach (var change in TripletChanges(new TripletsOperation.Update(index, type, from, to)))
        {
            if (change.Before is { } before)
            {
                return before;
            }
        }
        throw LinoProtocolException.Malformed($"the reply holds no change of {index}");
    }

    /// <inheritdoc/>
    public IReadOnlyList<Triplet<uint>> DeleteTriplet(uint index) =>
        TripletChanges(new TripletsOperation.Delete(index)).Where(change => change.Before is not null).Select(change => change.Before!.Value).ToList();

    /// <inheritdoc/>
    public IReadOnlyList<Triplet<uint>> EachTriplet(TripletQuery<uint> query) =>
        TripletsMatching(TripletsOperation.RestrictionOf(query));

    /// <inheritdoc/>
    public int CountTriplets(TripletQuery<uint> query) =>
        (int)CountTripletsMatching(TripletsOperation.RestrictionOf(query));

    /// <summary>How many other triplets use <paramref name="id"/>: the doublets behind them are not on the wire.</summary>
    public int DoubletUsages(uint id) =>
        EachTriplet(TripletQuery<uint>.ByType(id))
            .Concat(EachTriplet(TripletQuery<uint>.ByFrom(id)))
            .Concat(EachTriplet(TripletQuery<uint>.ByTo(id)))
            .Where(user => user.Index != id)
            .Distinct()
            .Count();

    private uint CreatedTriplet(TripletsOperation operation)
    {
        foreach (var change in TripletChanges(operation))
        {
            if (change.After is { } created)
            {
                return created.Index;
            }
        }
        throw LinoProtocolException.Malformed("the create reply holds no created triplet");
    }
}
