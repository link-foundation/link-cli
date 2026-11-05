using System.Globalization;
using Link.Foundation.Links.Notation.Binary;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// The triplets of a server in triplet mode (issue #152) as LiNo documents, so a store behind a
/// <see cref="LinksServer"/> is used exactly like a local <see cref="ITriplets{TLinkAddress}"/> store through
/// <see cref="RemoteLinks"/>.
/// </summary>
/// <remarks>
/// <para>
/// <c>(delete: i)</c>, <c>(count: ())</c> and a substitution query read the same in both worlds, so the shape of a
/// request cannot tell a triplet from a doublet. That is why triplets are a mode of the server,
/// <see cref="LinksServerOptions.Triplets"/> (<c>clink --serve --triplets</c>): a server without it answers exactly
/// as it always did, and one with it reads every request as triplets.
/// </para>
/// <list type="table">
/// <listheader><term>Request</term><description>Reply</description></listheader>
/// <item><term><c>(count: (index type from to))</c></term><description><c>(count: N)</c></description></item>
/// <item><term><c>(each: (index type from to))</c></term><description>one <c>(index: type from to)</c> per match</description></item>
/// <item><term><c>(create: (type from to))</c></term><description><c>() ((index: type from to))</c></description></item>
/// <item><term><c>(create: (0 0 0))</c></term><description><c>() ((index: index 0 0))</c>, a point: a triplet that is its own type</description></item>
/// <item><term><c>(update: (index type from to))</c></term><description><c>((index: t f t)) ((index: type from to))</c></description></item>
/// <item><term><c>(delete: index)</c></term><description><c>((index: type from to)) ()</c> per removed triplet, <c>index</c> first</description></item>
/// <item><term><c>(subscribe: (index type from to))</c></term><description><c>(subscription: N)</c>; later pushes of <c>(notify: N)</c> and its changes</description></item>
/// </list>
/// <para>
/// <c>get-name</c>, <c>set-name</c>, <c>get-by-name</c>, <c>remove-name</c>, <c>unsubscribe</c> and <c>auth</c> are the
/// same as without triplets (<see cref="LinksOperation"/>), and the empty request lists every triplet. Any other
/// document is a substitution query over triplets (<see cref="TripletsQueryProcessor"/>); its reply names the
/// references that have a name, like a query over doublets.
/// </para>
/// <para>
/// A restriction holds up to four parts, matched part by part: the parts it leaves out match anything, as does
/// <c>*</c>; an empty <c>from</c> or <c>to</c> is <c>0</c>. The wrappers and pairs that store a triplet in doublets
/// never appear.
/// </para>
/// <para>
/// A reply or push in a binary format whose arity holds three references but not the five of
/// <c>(index: type from to)</c>, such as <c>2..3</c>, carries every triplet as <c>(index: (type from to))</c>, which
/// travels as one link of three references; see <see cref="Pack"/>. Every parser here reads both forms.
/// </para>
/// </remarks>
public abstract record TripletsOperation
{
    private TripletsOperation() { }

    /// <summary>Number of triplets matching the restriction.</summary>
    public sealed record Count(LinksRestriction Restriction) : TripletsOperation
    {
        private protected override (string, LinoLink) Request => ("count", LinksOperation.RestrictionLino(Restriction));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(ITriplets<uint> triplets) =>
            new[] { LinksOperation.Named("count", LinksOperation.Number((uint)triplets.EachTriplet(Query(Restriction)).Count)) };
    }

    /// <summary>Every triplet matching the restriction, ordered by index.</summary>
    public sealed record Each(LinksRestriction Restriction) : TripletsOperation
    {
        private protected override (string, LinoLink) Request => ("each", LinksOperation.RestrictionLino(Restriction));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(ITriplets<uint> triplets) =>
            triplets.EachTriplet(Query(Restriction)).Select(TripletLino).ToList();
    }

    /// <summary>Creates a triplet; <c>(0 0 0)</c> creates a point, its own type.</summary>
    public sealed record Create(uint Type, uint From, uint To) : TripletsOperation
    {
        private protected override (string, LinoLink) Request => ("create", LinksOperation.Numbers(Type, From, To));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(ITriplets<uint> triplets)
        {
            Triplet<uint> created;
            if (Type == 0 && From == 0 && To == 0)
            {
                var point = triplets.CreateTripletPoint();
                created = new Triplet<uint>(point, point, 0, 0);
            }
            else
            {
                created = new Triplet<uint>(triplets.CreateTriplet(Type, From, To), Type, From, To);
            }
            return ChangesDocument(new[] { new TripletChange<uint>(null, created) });
        }
    }

    /// <summary>Makes an existing triplet <c>(index: type from to)</c>, keeping its address.</summary>
    public sealed record Update(uint Index, uint Type, uint From, uint To) : TripletsOperation
    {
        private protected override (string, LinoLink) Request => ("update", LinksOperation.Numbers(Index, Type, From, To));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(ITriplets<uint> triplets)
        {
            var before = triplets.UpdateTriplet(Index, Type, From, To);
            return ChangesDocument(new[] { new TripletChange<uint>(before, new Triplet<uint>(Index, Type, From, To)) });
        }
    }

    /// <summary>Deletes a triplet and every triplet that uses it.</summary>
    public sealed record Delete(uint Index) : TripletsOperation
    {
        private protected override (string, LinoLink) Request => ("delete", LinksOperation.Number(Index));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(ITriplets<uint> triplets) =>
            ChangesDocument(triplets.DeleteTriplet(Index).Select(triplet => new TripletChange<uint>(triplet, null)));
    }

    /// <summary>
    /// Asks a <see cref="LinksServer"/> to push the changes other connections make to the triplets matching the
    /// restriction; only the server can run it, so <see cref="Execute"/> on a store throws.
    /// </summary>
    public sealed record Subscribe(LinksRestriction Restriction) : TripletsOperation
    {
        private protected override (string, LinoLink) Request => ("subscribe", LinksOperation.RestrictionLino(Restriction));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(ITriplets<uint> triplets) => throw LinksOperation.NeedsServer();
    }

    private protected abstract (string Name, LinoLink Argument) Request { get; }

    /// <summary>Runs the operation against <paramref name="triplets"/> and returns the reply document.</summary>
    public abstract IReadOnlyList<LinoLink> Execute(ITriplets<uint> triplets);

    /// <summary>The request document for this operation.</summary>
    public IReadOnlyList<LinoLink> ToDocument() => new[] { LinksOperation.Named(Request.Name, Request.Argument) };

    /// <summary>
    /// Recognises a triplet operation; null for any other document, such as a substitution query or a request about names.
    /// </summary>
    /// <exception cref="LinoProtocolException">The document names an operation but its argument is malformed.</exception>
    public static TripletsOperation? FromDocument(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        if (document is not [{ Id: { } name, Values: [var argument] }])
        {
            return null;
        }
        switch (name)
        {
            case "count":
                return new Count(LinksOperation.ParseRestriction(argument, MaxParts));
            case "each":
                return new Each(LinksOperation.ParseRestriction(argument, MaxParts));
            case "create":
                var created = LinksOperation.ParseNumbers(argument, 3);
                return new Create(created[0], created[1], created[2]);
            case "update":
                var updated = LinksOperation.ParseNumbers(argument, 4);
                return new Update(updated[0], updated[1], updated[2], updated[3]);
            case "delete":
                return new Delete(LinksOperation.ParseNumber(argument));
            case "subscribe":
                return new Subscribe(LinksOperation.ParseRestriction(argument, MaxParts));
            default:
                return null;
        }
    }

    private const int MaxParts = 4;

    /// <summary>The query a restriction asks for; the parts it leaves out match anything.</summary>
    public static TripletQuery<uint> Query(LinksRestriction restriction)
    {
        ArgumentNullException.ThrowIfNull(restriction);
        uint? Part(int at) => at < restriction.Parts.Count ? restriction.Parts[at] : null;
        return new TripletQuery<uint>(Part(0), Part(1), Part(2), Part(3));
    }

    /// <summary>The restriction of <paramref name="query"/>, in the order of the wire.</summary>
    public static LinksRestriction RestrictionOf(TripletQuery<uint> query) =>
        LinksRestriction.Of(query.Index, query.Type, query.From, query.To);

    /// <summary>Whether <paramref name="triplet"/> matches <paramref name="restriction"/>, part by part.</summary>
    public static bool Matches(Triplet<uint> triplet, LinksRestriction restriction)
    {
        ArgumentNullException.ThrowIfNull(restriction);
        return restriction.Parts.Count <= MaxParts && Query(restriction).Matches(triplet);
    }

    /// <summary>
    /// Answers one request of a server in triplet mode against <paramref name="triplets"/>; failures produce an
    /// <see cref="LinksServer.ErrorDocument"/>. The counterpart of <see cref="LinksServer.ExecuteRequest(INamedTypesLinks{uint}, IReadOnlyList{LinoLink}, bool)"/>.
    /// </summary>
    public static IReadOnlyList<LinoLink> ExecuteRequest(ITriplets<uint> triplets, IReadOnlyList<LinoLink> document) =>
        ExecuteRequest(triplets, document, null);

    /// <summary>
    /// Like <see cref="ExecuteRequest(ITriplets{uint}, IReadOnlyList{LinoLink})"/>, adding every triplet the request
    /// changed to <paramref name="changed"/>, with plain numbers.
    /// </summary>
    internal static IReadOnlyList<LinoLink> ExecuteRequest(ITriplets<uint> triplets, IReadOnlyList<LinoLink> document, List<TripletChange<uint>>? changed)
    {
        ArgumentNullException.ThrowIfNull(triplets);
        ArgumentNullException.ThrowIfNull(document);
        try
        {
            if (FromDocument(document) is { } operation)
            {
                var reply = operation.Execute(triplets);
                if (operation is Create or Update or Delete)
                {
                    changed?.AddRange(ParseChanges(reply).Where(change => change.Before != change.After));
                }
                return reply;
            }
            if (LinksOperation.FromDocument(document) is { } links)
            {
                return links.Execute(triplets);
            }
            if (document.Count == 0)
            {
                return triplets.EachTriplet(TripletQuery<uint>.Any).Select(triplet => NamedTripletLino(triplets, triplet)).ToList();
            }
            var changes = TripletsQueryProcessor.ProcessQuery(triplets, LinoFormat.FormatDocument(document));
            changed?.AddRange(changes.Where(change => change.Before != change.After));
            return changes
                .Select(change => LinksOperation.Group(new[] { NamedSide(triplets, change.Before), NamedSide(triplets, change.After) }))
                .ToList();
        }
        catch (Exception error) when (error is not OutOfMemoryException)
        {
            return LinksServer.ErrorDocument(error.Message);
        }
    }

    /// <summary><c>(index: type from to)</c> with plain numbers.</summary>
    public static LinoLink TripletLino(Triplet<uint> triplet) =>
        LinksOperation.Named(
            triplet.Index.ToString(CultureInfo.InvariantCulture),
            LinksOperation.Number(triplet.Type),
            LinksOperation.Number(triplet.From),
            LinksOperation.Number(triplet.To));

    /// <summary>A reply listing <paramref name="changes"/>, one <c>(before) (after)</c> line each.</summary>
    public static IReadOnlyList<LinoLink> ChangesDocument(IEnumerable<TripletChange<uint>> changes) =>
        changes.Select(change => LinksOperation.Group(new[] { Side(change.Before), Side(change.After) })).ToList();

    /// <summary>A push: <c>(notify: N)</c> followed by one <c>(before) (after)</c> line per change.</summary>
    public static IReadOnlyList<LinoLink> NotificationDocument(uint subscription, IEnumerable<TripletChange<uint>> changes) =>
        ChangesDocument(changes).Prepend(LinksOperation.Named("notify", LinksOperation.Number(subscription))).ToList();

    /// <summary>The subscription and the changes of a <see cref="NotificationDocument"/>.</summary>
    public static TripletsNotification ParseNotification(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return LinksOperation.IsNotification(document) && document[0].Values is [var subscription]
            ? new TripletsNotification(LinksOperation.ParseNumber(subscription), ParseChanges(document.Skip(1).ToList()))
            : throw LinksOperation.MalformedReply("notify", document);
    }

    /// <summary>The triplets of an <c>each</c> reply.</summary>
    public static IReadOnlyList<Triplet<uint>> ParseTriplets(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document.Select(ParseTriplet).ToList();
    }

    /// <summary>The <c>(before) (after)</c> pairs of a <c>create</c>, <c>update</c> or <c>delete</c> reply.</summary>
    public static IReadOnlyList<TripletChange<uint>> ParseChanges(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document
            .Select(change => LinksOperation.Parts(change) is [var before, var after]
                ? new TripletChange<uint>(ParseSide(before), ParseSide(after))
                : throw LinksOperation.MalformedReply("change", document))
            .ToList();
    }

    /// <summary>A triplet, as <c>(index: type from to)</c> or as <see cref="Pack"/> writes it.</summary>
    public static Triplet<uint> ParseTriplet(LinoLink link)
    {
        if (link is { Id: { } index, Values: { } values })
        {
            IReadOnlyList<LinoLink> parts = values is [{ Id: null, Values: { } packed }] ? packed.ToList() : values.ToList();
            if (parts is [var type, var from, var to])
            {
                return new Triplet<uint>(
                    LinksOperation.ParseNumber(index),
                    LinksOperation.ParseNumber(type),
                    LinksOperation.ParseNumber(from),
                    LinksOperation.ParseNumber(to));
            }
        }
        throw LinoProtocolException.Malformed($"expected (index: type from to), found {LinoFormat.FormatLink(link)}");
    }

    /// <summary>
    /// Rewrites every triplet of <paramref name="document"/> so it fits <paramref name="format"/>: in a binary format
    /// whose arity holds three references but not five, <c>(index: type from to)</c> becomes
    /// <c>(index: (type from to))</c>, whose second part travels as one link of three references. Under any other
    /// format the document stays as it is.
    /// </summary>
    /// <remarks>
    /// <c>(index: type from to)</c> needs a link of five references, the marker of a named link and the index included,
    /// so under <c>2..3</c> the mapping would spell it as a chain of doublets: nothing would gain from the arity.
    /// </remarks>
    public static IReadOnlyList<LinoLink> Pack(IReadOnlyList<LinoLink> document, MessageFormat format)
    {
        ArgumentNullException.ThrowIfNull(document);
        return format.IsBinary && format.Options.Arity.Contains(3) && !format.Options.Arity.Contains(5)
            ? document.Select(PackLink).ToList()
            : document;
    }

    /// <summary>The inverse of <see cref="Pack"/>: the canonical text of a triplet reply does not depend on the format that carried it.</summary>
    public static IReadOnlyList<LinoLink> Canonical(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document.Select(UnpackLink).ToList();
    }

    private static LinoLink PackLink(LinoLink link) => link switch
    {
        { Id: null, Values: { } values } => LinksOperation.Group(values.Select(PackLink)),
        { Id: { } id, Values: { Count: 3 } values } => LinksOperation.Named(id, LinksOperation.Group(values)),
        _ => link,
    };

    private static LinoLink UnpackLink(LinoLink link) => link switch
    {
        { Id: null, Values: { } values } => LinksOperation.Group(values.Select(UnpackLink)),
        { Id: { } id, Values: [{ Id: null, Values: { Count: 3 } packed }] } => LinksOperation.Named(id, packed.ToArray()),
        _ => link,
    };

    /// <summary><c>(index: type from to)</c>, naming every reference that has a name.</summary>
    private static LinoLink NamedTripletLino(ITriplets<uint> triplets, Triplet<uint> triplet) =>
        LinksOperation.Named(
            LinksServer.ReferenceName(triplets, triplet.Index),
            LinoFormat.Reference(LinksServer.ReferenceName(triplets, triplet.Type)),
            LinoFormat.Reference(LinksServer.ReferenceName(triplets, triplet.From)),
            LinoFormat.Reference(LinksServer.ReferenceName(triplets, triplet.To)));

    private static LinoLink NamedSide(ITriplets<uint> triplets, Triplet<uint>? triplet) =>
        LinksOperation.Group(triplet is { } some ? new[] { NamedTripletLino(triplets, some) } : Array.Empty<LinoLink>());

    private static LinoLink Side(Triplet<uint>? triplet) =>
        LinksOperation.Group(triplet is { } some ? new[] { TripletLino(some) } : Array.Empty<LinoLink>());

    private static Triplet<uint>? ParseSide(LinoLink side) => side switch
    {
        { Id: null, Values: [] } => null,
        { Id: null, Values: [var link] } => ParseTriplet(link),
        // `((index: type from to))` loses its wrapper when the side is the single named link itself.
        { Id: not null, Values: not null } => ParseTriplet(side),
        _ => throw LinksOperation.MalformedArgument("a change side", side),
    };
}

/// <summary>
/// A change a <see cref="LinksServer"/> in triplet mode pushed to a subscriber; a side that does not fit the
/// restriction (or that the subscriber may not read) is null.
/// </summary>
public sealed record TripletsNotification(uint Subscription, IReadOnlyList<TripletChange<uint>> Changes)
{
    /// <inheritdoc/>
    public bool Equals(TripletsNotification? other) =>
        other is not null && Subscription == other.Subscription && Changes.SequenceEqual(other.Changes);

    /// <inheritdoc/>
    public override int GetHashCode() => HashCode.Combine(Subscription, Changes.Count);
}
