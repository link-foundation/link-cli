using System.Globalization;
using System.Numerics;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Delegates;
using Platform.Data.Doublets;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// The parts of a link a <see cref="LinksOperation{T}.Count"/> or <see cref="LinksOperation{T}.Each"/> asks for;
/// a null part matches any value.
/// </summary>
/// <remarks>
/// Matched like the raw links interface matches a query: no parts match every link, <c>(index)</c> one link,
/// <c>(index value)</c> links of that index whose source or target is <c>value</c>, and
/// <c>(index source target)</c> matches part by part. Every link is matched at most once, so a link that
/// uses <c>value</c> as both source and target is counted once, where <c>UnitedMemoryLinks</c> counts its
/// two usages.
/// </remarks>
public sealed record LinksRestriction<T>(IReadOnlyList<T?> Parts)
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    /// <summary>The wire spelling of a part that matches any value.</summary>
    public const string AnyPart = "*";

    /// <summary>Matches every link.</summary>
    public static LinksRestriction<T> All { get; } = new(Array.Empty<T?>());

    /// <summary>A restriction of the given parts.</summary>
    public static LinksRestriction<T> Of(params T?[] parts) => new(parts);

    /// <summary>Whether <paramref name="link"/> matches this restriction.</summary>
    public bool Matches(Link<T> link)
    {
        static bool Is(T? part, T value) => part is not { } expected || expected == value;
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
    public bool Equals(LinksRestriction<T>? other) => other is not null && Parts.SequenceEqual(other.Parts);

    /// <inheritdoc/>
    public override int GetHashCode() => Parts.Aggregate(Parts.Count, HashCode.Combine);

    /// <summary>The restriction as it is sent: <c>(index source target)</c> with <c>*</c> for any part.</summary>
    public override string ToString() => LinoFormat.FormatLink(LinksOperation<T>.Group(Parts.Select(PartLino)));

    internal static LinoLink PartLino(T? part) => part is { } value ? LinksOperation<T>.Number(value) : LinoFormat.Reference(AnyPart);
}

/// <summary>
/// One call of the links interface (<see cref="INamedTypesLinks{TLinkAddress}"/>) as a LiNo document, so a store
/// behind a <see cref="LinksServer"/> is used exactly like a local one through <see cref="RemoteLinks"/>.
/// </summary>
/// <remarks>
/// <para>
/// Every request is one top-level link named after the operation and holding exactly one value. The substitution
/// query language ignores that shape (a query needs a restriction <em>and</em> a substitution), so an operation
/// never collides with a query sent to the same server. The Rust <c>LinksOperation&lt;T&gt;</c> uses the same documents,
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
public abstract record LinksOperation<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private LinksOperation() { }

    /// <summary>Number of links matching the restriction.</summary>
    public sealed record Count(LinksRestriction<T> Restriction) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("count", RestrictionLino(Restriction));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links) =>
            new[] { Named("count", Number(T.CreateChecked(Matching(links, Restriction).Count))) };
    }

    /// <summary>Every link matching the restriction, ordered by index.</summary>
    public sealed record Each(LinksRestriction<T> Restriction) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("each", RestrictionLino(Restriction));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links) =>
            Matching(links, Restriction).Select(LinkLino).ToList();
    }

    /// <summary>Creates a link.</summary>
    /// <remarks>
    /// A store that keeps doublets unique merges a link created as an existing doublet into it, and the reply then
    /// holds no creation.
    /// </remarks>
    public sealed record Create(T Source, T Target) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("create", Numbers(Source, Target));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links)
        {
            var changes = Recorded(links, record => links.Create(null, record));
            if (Source != T.Zero || Target != T.Zero)
            {
                var index = changes[^1].After.Index;
                changes.AddRange(Recorded(links, record =>
                    links.Update([index], new LinkParts<T>(index, Source, Target), record)));
            }
            return NetChangesDocument(changes);
        }
    }

    /// <summary>
    /// Points an existing link at a new source and target; an update that would duplicate an existing link
    /// merges into it instead.
    /// </summary>
    public sealed record Update(T Index, T Source, T Target) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("update", Numbers(Index, Source, Target));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links)
        {
            ThrowIfMissing(links, Index);
            return NetChangesDocument(Recorded(links, record =>
                links.Update([Index], new LinkParts<T>(Index, Source, Target), record)));
        }
    }

    /// <summary>Deletes a link, and every link that still refers to it.</summary>
    public sealed record Delete(T Index) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("delete", Number(Index));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links)
        {
            ThrowIfMissing(links, Index);
            return NetChangesDocument(Recorded(links, record => links.Delete([Index], record)));
        }
    }

    /// <summary>The name of a link.</summary>
    public sealed record GetName(T Link) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("get-name", Number(Link));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links) =>
            links.GetName(Link) is { } name ? new[] { Named("name", LinoFormat.Reference(name)) } : Array.Empty<LinoLink>();
    }

    /// <summary>Names a link.</summary>
    public sealed record SetName(T Link, string Name) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request =>
            ("set-name", Group(new[] { Number(Link), LinoFormat.Reference(Name) }));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links) => LinkReply(links.SetName(Link, Name));
    }

    /// <summary>The link with a name.</summary>
    public sealed record GetByName(string Name) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("get-by-name", LinoFormat.Reference(Name));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links) => LinkReply(links.GetByName(Name));
    }

    /// <summary>Removes the name of a link.</summary>
    public sealed record RemoveName(T Link) : LinksOperation<T>
    {
        private protected override (string, LinoLink) Request => ("remove-name", Number(Link));

        /// <inheritdoc/>
        private protected override IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links)
        {
            links.RemoveName(Link);
            return Array.Empty<LinoLink>();
        }
    }

    /// <summary>The operation name and its one argument.</summary>
    private protected abstract (string Name, LinoLink Argument) Request { get; }

    /// <summary>Runs the operation against <paramref name="links"/> and returns the reply document.</summary>
    /// <exception cref="ArgumentException">An update or delete names a link that does not exist.</exception>
    public IReadOnlyList<LinoLink> Execute(INamedTypesLinks<T> links)
    {
        ArgumentNullException.ThrowIfNull(links);
        var limits = links.QueryLimits;
        limits?.Document(ToDocument());
        IEnumerable<T> addresses = this switch
        {
            Count operation => operation.Restriction.Parts.OfType<T>(),
            Each operation => operation.Restriction.Parts.OfType<T>(),
            Create operation => [operation.Source, operation.Target],
            Update operation => [operation.Index, operation.Source, operation.Target],
            Delete operation => [operation.Index],
            GetName operation => [operation.Link],
            SetName operation => [operation.Link],
            RemoveName operation => [operation.Link],
            _ => []
        };
        if (limits is not null) foreach (var address in addresses)
            if (UInt128.CreateChecked(address) > limits.MaxAddress) throw new ArgumentOutOfRangeException(nameof(address));
        var reply = ExecuteCore(links);
        limits?.DecodedReply(reply);
        return reply;
    }

    private protected abstract IReadOnlyList<LinoLink> ExecuteCore(INamedTypesLinks<T> links);

    /// <summary>The request document for this operation.</summary>
    public IReadOnlyList<LinoLink> ToDocument() => new[] { Named(Request.Name, Request.Argument) };

    /// <summary>
    /// Recognises an operation request; null for any other document, such as a substitution query.
    /// </summary>
    /// <exception cref="LinoProtocolException">The document names an operation but its argument is malformed.</exception>
    public static LinksOperation<T>? FromDocument(IReadOnlyList<LinoLink> document)
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
    public static T ParseCount(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document is [{ Id: "count", Values: [var count] }]
            ? ParseNumber(count)
            : throw MalformedReply("count", document);
    }

    /// <summary>The links of an <c>each</c> reply.</summary>
    public static IReadOnlyList<Link<T>> ParseLinks(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document.Select(ParseLink).ToList();
    }

    /// <summary>
    /// The <c>(before) (after)</c> pairs of a <c>create</c>, <c>update</c> or <c>delete</c> reply; a missing side is
    /// the null link.
    /// </summary>
    public static IReadOnlyList<(Link<T> Before, Link<T> After)> ParseChanges(IReadOnlyList<LinoLink> document)
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
    public static T? ParseLinkReply(IReadOnlyList<LinoLink> document)
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
    public static IReadOnlyList<LinoLink> ChangesDocument(IEnumerable<(Link<T> Before, Link<T> After)> changes) =>
        changes.Select(change => ChangeLino(change.Before, change.After, LinkLino)).ToList();

    /// <summary>
    /// <c>(before) (after)</c>, where a side is <c>()</c> for the null link and <c>((index: source target))</c>
    /// otherwise, with every link written by <paramref name="linkLino"/>.
    /// </summary>
    internal static LinoLink ChangeLino(Link<T> before, Link<T> after, Func<Link<T>, LinoLink> linkLino)
    {
        LinoLink Side(Link<T> link) => Group((link.Index == T.Zero && link.Source == T.Zero && link.Target == T.Zero) ? Array.Empty<LinoLink>() : new[] { linkLino(link) });
        return Group(new[] { Side(before), Side(after) });
    }

    /// <summary><c>(index: source target)</c> with plain numbers.</summary>
    public static LinoLink LinkLino(Link<T> link) =>
        Named(link.Index.ToString(null, CultureInfo.InvariantCulture), Number(link.Source), Number(link.Target));

    internal static LinoLink Number(T value) => LinoFormat.Reference(value.ToString(null, CultureInfo.InvariantCulture));

    internal static LinoLink Group(IEnumerable<LinoLink> values) => LinoFormat.Link(null, values.ToList());

    private static LinoLink Named(string name, params LinoLink[] values) => LinoFormat.Link(name, values.ToList());

    private static LinoLink Numbers(params T[] numbers) => Group(numbers.Select(Number));

    private static LinoLink RestrictionLino(LinksRestriction<T> restriction) => Group(restriction.Parts.Select(LinksRestriction<T>.PartLino));

    private static IReadOnlyList<LinoLink> LinkReply(T link) =>
        link == T.Zero ? Array.Empty<LinoLink>() : new[] { Named("link", Number(link)) };

    private static List<Link<T>> Matching(INamedTypesLinks<T> links, LinksRestriction<T> restriction)
    {
        var any = links.Constants.Any;
        IList<T> candidates = restriction.Parts is [{ } index, ..] ? [index] : [any, any, any];
        var matched = new List<Link<T>>();
        links.Each(candidates, raw =>
        {
            var row = Owned(raw);
            if (restriction.Matches(row))
            {
                links.QueryLimits?.Results(matched.Count + 1);
                matched.Add(row);
            }
            return links.Constants.Continue;
        });
        return matched.OrderBy(link => link.Index).ToList();
    }

    /// <summary>
    /// The net change of every link a write touched: the steps a store takes to get there differ between the C# and
    /// the Rust stores, the net changes do not.
    /// </summary>
    private static IReadOnlyList<LinoLink> NetChangesDocument(List<(Link<T> Before, Link<T> After)> steps) =>
        ChangesDocument(ChangesSimplifier.SimplifyChanges(steps));

    /// <summary>Runs <paramref name="write"/> and returns every change it reports, in order.</summary>
    private static List<(Link<T> Before, Link<T> After)> Recorded(INamedTypesLinks<T> links, Action<WriteHandler<T>> write)
    {
        var changes = new List<(Link<T> Before, Link<T> After)>();
        write((before, after) =>
        {
            changes.Add((Owned(before), Owned(after)));
            return links.Constants.Continue;
        });
        return changes;
    }

    private static Link<T> Owned(IList<T>? row) => row is null ? default : new(row[0], row[1], row[2]);

    private static void ThrowIfMissing(INamedTypesLinks<T> links, T index)
    {
        if (Matching(links, LinksRestriction<T>.Of(index)).Count == 0)
        {
            throw new ArgumentException($"Link not found: {index}");
        }
    }

    private static Link<T> ParseChangeSide(LinoLink side) => side switch
    {
        { Id: null, Values: [] } => default,
        { Id: null, Values: [var link] } => ParseLink(link),
        // `((index: source target))` loses its wrapper when the side is the single named link itself.
        { Id: not null, Values: not null } => ParseLink(side),
        _ => throw LinoProtocolException.Malformed($"expected a change side, found {LinoFormat.FormatLink(side)}"),
    };

    private static Link<T> ParseLink(LinoLink link) => link is { Id: { } index, Values: [var source, var target] }
        ? new Link<T>(ParseNumber(index), ParseNumber(source), ParseNumber(target))
        : throw LinoProtocolException.Malformed($"expected (index: source target), found {LinoFormat.FormatLink(link)}");

    private static LinksRestriction<T> ParseRestriction(LinoLink argument)
    {
        var parts = Parts(argument);
        if (parts.Count > 3)
        {
            throw MalformedArgument("restriction", argument);
        }
        return new LinksRestriction<T>(parts
            .Select(part => part is { Values: null, Id: LinksRestriction<T>.AnyPart } ? null : (T?)ParseNumber(part))
            .ToList());
    }

    private static T[] ParseNumbers(LinoLink argument, int count)
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

    private static T ParseNumber(LinoLink value) => value is { Values: null, Id: { } text }
        ? ParseNumber(text)
        : throw LinoProtocolException.Malformed($"expected a number, found {LinoFormat.FormatLink(value)}");

    private static T ParseNumber(string text) =>
        T.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out var number)
            ? number
            : throw LinoProtocolException.Malformed($"expected a number, found '{text}'");

    private static LinoProtocolException MalformedArgument(string expected, LinoLink argument) =>
        LinoProtocolException.Malformed($"expected {expected}, found {LinoFormat.FormatLink(argument)}");

    private static LinoProtocolException MalformedReply(string expected, IReadOnlyList<LinoLink> document) =>
        LinoProtocolException.Malformed($"expected a {expected} reply, found {LinoFormat.FormatDocument(document)}");
}
