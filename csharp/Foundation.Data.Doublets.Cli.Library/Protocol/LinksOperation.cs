using System.Globalization;
using Platform.Data;
using Platform.Delegates;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// The parts of a link a <see cref="LinksOperation.Count"/> or <see cref="LinksOperation.Each"/> asks for;
/// a null part matches any value.
/// </summary>
/// <remarks>
/// Matched like the raw links interface matches a query: no parts match every link, <c>(index)</c> one link,
/// <c>(index value)</c> links of that index whose source or target is <c>value</c>, and
/// <c>(index source target)</c> matches part by part. Every link is matched at most once, so a link that
/// uses <c>value</c> as both source and target is counted once, where <c>UnitedMemoryLinks</c> counts its
/// two usages.
/// </remarks>
public sealed record LinksRestriction(IReadOnlyList<uint?> Parts)
{
    /// <summary>The wire spelling of a part that matches any value.</summary>
    public const string AnyPart = "*";

    /// <summary>Matches every link.</summary>
    public static LinksRestriction All { get; } = new(Array.Empty<uint?>());

    /// <summary>A restriction of the given parts.</summary>
    public static LinksRestriction Of(params uint?[] parts) => new(parts);

    /// <summary>Whether <paramref name="link"/> matches this restriction.</summary>
    public bool Matches(DoubletLink link)
    {
        static bool Is(uint? part, uint value) => part is not { } expected || expected == value;
        return Parts switch
        {
            [] => true,
            [var index] => Is(index, link.Index),
            [var index, var value] => Is(index, link.Index) && (Is(value, link.Source) || Is(value, link.Target)),
            [var index, var source, var target] => Is(index, link.Index) && Is(source, link.Source) && Is(target, link.Target),
            _ => false,
        };
    }

    /// <inheritdoc/>
    public bool Equals(LinksRestriction? other) => other is not null && Parts.SequenceEqual(other.Parts);

    /// <inheritdoc/>
    public override int GetHashCode() => Parts.Aggregate(Parts.Count, HashCode.Combine);

    /// <summary>The restriction as it is sent: <c>(index source target)</c> with <c>*</c> for any part.</summary>
    public override string ToString() => LinoFormat.FormatLink(LinksOperation.Group(Parts.Select(PartLino)));

    internal static LinoLink PartLino(uint? part) => part is { } value ? LinksOperation.Number(value) : LinoFormat.Reference(AnyPart);
}

/// <summary>
/// One call of the links interface (<see cref="INamedTypesLinks{TLinkAddress}"/>) as a LiNo document, so a store
/// behind a <see cref="LinksServer"/> is used exactly like a local one through <see cref="RemoteLinks"/>.
/// </summary>
/// <remarks>
/// <para>
/// Every request is one top-level link named after the operation and holding exactly one value. The substitution
/// query language ignores that shape (a query needs a restriction <em>and</em> a substitution), so an operation
/// never collides with a query sent to the same server. The Rust <c>LinksOperation</c> uses the same documents,
/// so clients and servers of both ports talk to each other.
/// </para>
/// <list type="table">
/// <listheader><term>Request</term><description>Reply</description></listheader>
/// <item><term><c>(count: (index source target))</c></term><description><c>(count: N)</c></description></item>
/// <item><term><c>(each: (index source target))</c></term><description>one <c>(index: source target)</c> per match, ordered by index</description></item>
/// <item><term><c>(create: (source target))</c></term><description><c>() ((index: source target))</c></description></item>
/// <item><term><c>(update: (index source target))</c></term><description><c>((index: s t)) ((index: source target))</c> per change</description></item>
/// <item><term><c>(delete: index)</c></term><description><c>((index: source target)) ()</c> per removed link</description></item>
/// <item><term><c>(get-name: link)</c></term><description><c>(name: 'text')</c>, or nothing</description></item>
/// <item><term><c>(set-name: (link 'text'))</c></term><description><c>(link: N)</c></description></item>
/// <item><term><c>(get-by-name: 'text')</c></term><description><c>(link: N)</c>, or nothing</description></item>
/// <item><term><c>(remove-name: link)</c></term><description>nothing</description></item>
/// </list>
/// <para>
/// Every reference in a reply is a number; failures are <c>(error: 'message')</c>. The changes of a
/// <c>create</c>, <c>update</c> or <c>delete</c> are the net change of every link it touched, the way
/// <c>clink --changes</c> reports a query: a cascading delete is one <c>((index: source target)) ()</c> per
/// removed link, without the intermediate steps by which the store behind the server got there.
/// </para>
/// </remarks>
public abstract record LinksOperation
{
    private LinksOperation() { }

    /// <summary>Number of links matching the restriction.</summary>
    public sealed record Count(LinksRestriction Restriction) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("count", RestrictionLino(Restriction));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links) =>
            new[] { Named("count", Number((uint)Matching(links, Restriction).Count)) };
    }

    /// <summary>Every link matching the restriction, ordered by index.</summary>
    public sealed record Each(LinksRestriction Restriction) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("each", RestrictionLino(Restriction));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links) =>
            Matching(links, Restriction).Select(LinkLino).ToList();
    }

    /// <summary>Creates a link.</summary>
    /// <remarks>
    /// A store that keeps doublets unique merges a link created as an existing doublet into it, and the reply then
    /// holds no creation.
    /// </remarks>
    public sealed record Create(uint Source, uint Target) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("create", Numbers(Source, Target));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links)
        {
            var changes = Recorded(links, record => links.Create(null, record));
            if (Source != 0 || Target != 0)
            {
                var index = changes[^1].After.Index;
                changes.AddRange(Recorded(links, record =>
                    links.Update(new LinkAddress<uint>(index), new DoubletLink(index, Source, Target), record)));
            }
            return NetChangesDocument(changes);
        }
    }

    /// <summary>
    /// Points an existing link at a new source and target; an update that would duplicate an existing link
    /// merges into it instead.
    /// </summary>
    public sealed record Update(uint Index, uint Source, uint Target) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("update", Numbers(Index, Source, Target));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links)
        {
            ThrowIfMissing(links, Index);
            return NetChangesDocument(Recorded(links, record =>
                links.Update(new LinkAddress<uint>(Index), new DoubletLink(Index, Source, Target), record)));
        }
    }

    /// <summary>Deletes a link, and every link that still refers to it.</summary>
    public sealed record Delete(uint Index) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("delete", Number(Index));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links)
        {
            ThrowIfMissing(links, Index);
            return NetChangesDocument(Recorded(links, record => links.Delete(new LinkAddress<uint>(Index), record)));
        }
    }

    /// <summary>The name of a link.</summary>
    public sealed record GetName(uint Link) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("get-name", Number(Link));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links) =>
            links.GetName(Link) is { } name ? new[] { Named("name", LinoFormat.Reference(name)) } : Array.Empty<LinoLink>();
    }

    /// <summary>Names a link.</summary>
    public sealed record SetName(uint Link, string Name) : LinksOperation
    {
        private protected override (string, LinoLink) Request =>
            ("set-name", Group(new[] { Number(Link), LinoFormat.Reference(Name) }));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links) => LinkReply(links.SetName(Link, Name));
    }

    /// <summary>The link with a name.</summary>
    public sealed record GetByName(string Name) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("get-by-name", LinoFormat.Reference(Name));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links) => LinkReply(links.GetByName(Name));
    }

    /// <summary>Removes the name of a link.</summary>
    public sealed record RemoveName(uint Link) : LinksOperation
    {
        private protected override (string, LinoLink) Request => ("remove-name", Number(Link));

        /// <inheritdoc/>
        public override IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links)
        {
            links.RemoveName(Link);
            return Array.Empty<LinoLink>();
        }
    }

    /// <summary>The operation name and its one argument.</summary>
    private protected abstract (string Name, LinoLink Argument) Request { get; }

    /// <summary>Runs the operation against <paramref name="links"/> and returns the reply document.</summary>
    /// <exception cref="ArgumentException">An update or delete names a link that does not exist.</exception>
    public abstract IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links);

    /// <summary>The request document for this operation.</summary>
    public IReadOnlyList<LinoLink> ToDocument() => new[] { Named(Request.Name, Request.Argument) };

    /// <summary>
    /// Recognises an operation request; null for any other document, such as a substitution query.
    /// </summary>
    /// <exception cref="LinoProtocolException">The document names an operation but its argument is malformed.</exception>
    public static LinksOperation? FromDocument(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        if (document is not [{ Id: { } name, Values: [var argument] }])
        {
            return null;
        }
        switch (name)
        {
            case "count":
                return new Count(ParseRestriction(argument));
            case "each":
                return new Each(ParseRestriction(argument));
            case "create":
                var created = ParseNumbers(argument, 2);
                return new Create(created[0], created[1]);
            case "update":
                var updated = ParseNumbers(argument, 3);
                return new Update(updated[0], updated[1], updated[2]);
            case "delete":
                return new Delete(ParseNumber(argument));
            case "get-name":
                return new GetName(ParseNumber(argument));
            case "set-name":
                return Parts(argument) is [var link, { Values: null, Id: { } text }]
                    ? new SetName(ParseNumber(link), text)
                    : throw MalformedArgument("set-name", argument);
            case "get-by-name":
                return argument is { Values: null, Id: { } wanted }
                    ? new GetByName(wanted)
                    : throw MalformedArgument("get-by-name", argument);
            case "remove-name":
                return new RemoveName(ParseNumber(argument));
            default:
                return null;
        }
    }

    /// <summary>The count of a <c>(count: N)</c> reply.</summary>
    public static uint ParseCount(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document is [{ Id: "count", Values: [var count] }]
            ? ParseNumber(count)
            : throw MalformedReply("count", document);
    }

    /// <summary>The links of an <c>each</c> reply.</summary>
    public static IReadOnlyList<DoubletLink> ParseLinks(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document.Select(ParseLink).ToList();
    }

    /// <summary>
    /// The <c>(before) (after)</c> pairs of a <c>create</c>, <c>update</c> or <c>delete</c> reply; a missing side is
    /// the null link.
    /// </summary>
    public static IReadOnlyList<(DoubletLink Before, DoubletLink After)> ParseChanges(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document
            .Select(change => Parts(change) is [var before, var after]
                ? (ParseChangeSide(before), ParseChangeSide(after))
                : throw MalformedReply("change", document))
            .ToList();
    }

    /// <summary>The name of a <c>get-name</c> reply, or null.</summary>
    public static string? ParseName(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document switch
        {
            [] => null,
            [{ Id: "name", Values: [{ Values: null, Id: var name }] }] => name,
            _ => throw MalformedReply("name", document),
        };
    }

    /// <summary>The link of a <c>set-name</c> or <c>get-by-name</c> reply, or null.</summary>
    public static uint? ParseLinkReply(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document switch
        {
            [] => null,
            [{ Id: "link", Values: [var link] }] => ParseNumber(link),
            _ => throw MalformedReply("link", document),
        };
    }

    /// <summary>A reply listing <paramref name="changes"/>, one <c>(before) (after)</c> line each.</summary>
    public static IReadOnlyList<LinoLink> ChangesDocument(IEnumerable<(DoubletLink Before, DoubletLink After)> changes) =>
        changes.Select(change => ChangeLino(change.Before, change.After, LinkLino)).ToList();

    /// <summary>
    /// <c>(before) (after)</c>, where a side is <c>()</c> for the null link and <c>((index: source target))</c>
    /// otherwise, with every link written by <paramref name="linkLino"/>.
    /// </summary>
    internal static LinoLink ChangeLino(DoubletLink before, DoubletLink after, Func<DoubletLink, LinoLink> linkLino)
    {
        LinoLink Side(DoubletLink link) => Group(link.IsNull() ? Array.Empty<LinoLink>() : new[] { linkLino(link) });
        return Group(new[] { Side(before), Side(after) });
    }

    /// <summary><c>(index: source target)</c> with plain numbers.</summary>
    public static LinoLink LinkLino(DoubletLink link) =>
        Named(link.Index.ToString(CultureInfo.InvariantCulture), Number(link.Source), Number(link.Target));

    internal static LinoLink Number(uint value) => LinoFormat.Reference(value.ToString(CultureInfo.InvariantCulture));

    internal static LinoLink Group(IEnumerable<LinoLink> values) => LinoFormat.Link(null, values.ToList());

    private static LinoLink Named(string name, params LinoLink[] values) => LinoFormat.Link(name, values.ToList());

    private static LinoLink Numbers(params uint[] numbers) => Group(numbers.Select(Number));

    private static LinoLink RestrictionLino(LinksRestriction restriction) => Group(restriction.Parts.Select(LinksRestriction.PartLino));

    private static IReadOnlyList<LinoLink> LinkReply(uint link) =>
        link == 0 ? Array.Empty<LinoLink>() : new[] { Named("link", Number(link)) };

    private static List<DoubletLink> Matching(INamedTypesLinks<uint> links, LinksRestriction restriction)
    {
        var any = links.Constants.Any;
        var candidates = restriction.Parts is [{ } index, ..]
            ? (links.Exists(index) ? new[] { new DoubletLink(links.GetLink(index)) } : Array.Empty<DoubletLink>())
            : links.All(new DoubletLink(any, any, any)).Select(link => new DoubletLink(link));
        return candidates.Where(restriction.Matches).OrderBy(link => link.Index).ToList();
    }

    /// <summary>
    /// The net change of every link a write touched: the steps a store takes to get there differ between the C# and
    /// the Rust stores, the net changes do not.
    /// </summary>
    private static IReadOnlyList<LinoLink> NetChangesDocument(List<(DoubletLink Before, DoubletLink After)> steps) =>
        ChangesDocument(ChangesSimplifier.SimplifyChanges(steps));

    /// <summary>Runs <paramref name="write"/> and returns every change it reports, in order.</summary>
    private static List<(DoubletLink Before, DoubletLink After)> Recorded(INamedTypesLinks<uint> links, Action<WriteHandler<uint>> write)
    {
        var changes = new List<(DoubletLink Before, DoubletLink After)>();
        write((before, after) =>
        {
            changes.Add((new DoubletLink(before), new DoubletLink(after)));
            return links.Constants.Continue;
        });
        return changes;
    }

    private static void ThrowIfMissing(INamedTypesLinks<uint> links, uint index)
    {
        if (!links.Exists(index))
        {
            throw new ArgumentException($"Link not found: {index}");
        }
    }

    private static DoubletLink ParseChangeSide(LinoLink side) => side switch
    {
        { Id: null, Values: [] } => default,
        { Id: null, Values: [var link] } => ParseLink(link),
        // `((index: source target))` loses its wrapper when the side is the single named link itself.
        { Id: not null, Values: not null } => ParseLink(side),
        _ => throw LinoProtocolException.Malformed($"expected a change side, found {LinoFormat.FormatLink(side)}"),
    };

    private static DoubletLink ParseLink(LinoLink link) => link is { Id: { } index, Values: [var source, var target] }
        ? new DoubletLink(ParseNumber(index), ParseNumber(source), ParseNumber(target))
        : throw LinoProtocolException.Malformed($"expected (index: source target), found {LinoFormat.FormatLink(link)}");

    private static LinksRestriction ParseRestriction(LinoLink argument)
    {
        var parts = Parts(argument);
        if (parts.Count > 3)
        {
            throw MalformedArgument("restriction", argument);
        }
        return new LinksRestriction(parts
            .Select(part => part is { Values: null, Id: LinksRestriction.AnyPart } ? null : (uint?)ParseNumber(part))
            .ToList());
    }

    private static uint[] ParseNumbers(LinoLink argument, int count)
    {
        var numbers = Parts(argument).Select(ParseNumber).ToArray();
        return numbers.Length == count ? numbers : throw MalformedArgument($"{count} numbers", argument);
    }

    /// <summary>
    /// The values of an unnamed group; a lone reference is a group of one, since the canonical document model
    /// unwraps <c>(x)</c> to <c>x</c>.
    /// </summary>
    private static IReadOnlyList<LinoLink> Parts(LinoLink argument) =>
        argument is { Id: null, Values: { } values } ? values.ToList() : new[] { argument };

    private static uint ParseNumber(LinoLink value) => value is { Values: null, Id: { } text }
        ? ParseNumber(text)
        : throw LinoProtocolException.Malformed($"expected a number, found {LinoFormat.FormatLink(value)}");

    private static uint ParseNumber(string text) =>
        uint.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out var number)
            ? number
            : throw LinoProtocolException.Malformed($"expected a number, found '{text}'");

    private static LinoProtocolException MalformedArgument(string expected, LinoLink argument) =>
        LinoProtocolException.Malformed($"expected {expected}, found {LinoFormat.FormatLink(argument)}");

    private static LinoProtocolException MalformedReply(string expected, IReadOnlyList<LinoLink> document) =>
        LinoProtocolException.Malformed($"expected a {expected} reply, found {LinoFormat.FormatDocument(document)}");
}
