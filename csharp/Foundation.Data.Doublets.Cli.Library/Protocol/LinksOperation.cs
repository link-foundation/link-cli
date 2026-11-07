using Link.Foundation.Links.Notation.Binary;
using System.Globalization;
using Platform.Data.Doublets;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>The established uint restriction; null parts match any value.</summary>
public sealed record LinksRestriction(IReadOnlyList<uint?> Parts)
{
    public const string AnyPart = LinksRestriction<uint>.AnyPart;
    public static LinksRestriction All { get; } = new(Array.Empty<uint?>());
    public static LinksRestriction Of(params uint?[] parts) => new(parts);
    internal LinksRestriction<uint> Selected => new(Parts);
    public bool Matches(Link<uint> link) => Selected.Matches(link);
    public bool Equals(LinksRestriction? other) => other is not null && Parts.SequenceEqual(other.Parts);
    public override int GetHashCode() => Parts.Aggregate(Parts.Count, HashCode.Combine);
    public override string ToString() => Selected.ToString();
    internal static LinoLink PartLino(uint? part) => LinksRestriction<uint>.PartLino(part);
}

/// <summary>The established uint operation facade over selected-address protocol algorithms.</summary>
public abstract record LinksOperation
{
    private LinksOperation() { }
    private protected abstract LinksOperation<uint> Selected { get; }
    public sealed record Count(LinksRestriction Restriction) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.Count(Restriction.Selected); }
    public sealed record Each(LinksRestriction Restriction) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.Each(Restriction.Selected); }
    public sealed record Create(uint Source, uint Target) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.Create(Source, Target); }
    public sealed record Update(uint Index, uint Source, uint Target) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.Update(Index, Source, Target); }
    public sealed record Delete(uint Index) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.Delete(Index); }
    public sealed record GetName(uint Link) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.GetName(Link); }
    public sealed record SetName(uint Link, string Name) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.SetName(Link, Name); }
    public sealed record GetByName(string Name) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.GetByName(Name); }
    public sealed record RemoveName(uint Link) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.RemoveName(Link); }

    public sealed record Subscribe(LinksRestriction Restriction) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.Subscribe(Restriction.Selected); }
    public sealed record Unsubscribe(uint Subscription) : LinksOperation
    { private protected override LinksOperation<uint> Selected => new LinksOperation<uint>.Unsubscribe(Subscription); }

    public IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links) => Selected.Execute(links);
    public IReadOnlyList<LinoLink> ToDocument() => Selected.ToDocument();
    public static LinksOperation? FromDocument(IReadOnlyList<LinoLink> document) => LinksOperation<uint>.FromDocument(document) switch
    {
        LinksOperation<uint>.Count operation => new Count(new(operation.Restriction.Parts)),
        LinksOperation<uint>.Each operation => new Each(new(operation.Restriction.Parts)),
        LinksOperation<uint>.Create operation => new Create(operation.Source, operation.Target),
        LinksOperation<uint>.Update operation => new Update(operation.Index, operation.Source, operation.Target),
        LinksOperation<uint>.Delete operation => new Delete(operation.Index),
        LinksOperation<uint>.GetName operation => new GetName(operation.Link),
        LinksOperation<uint>.SetName operation => new SetName(operation.Link, operation.Name),
        LinksOperation<uint>.GetByName operation => new GetByName(operation.Name),
        LinksOperation<uint>.RemoveName operation => new RemoveName(operation.Link),
        LinksOperation<uint>.Subscribe operation => new Subscribe(new(operation.Restriction.Parts)),
        LinksOperation<uint>.Unsubscribe operation => new Unsubscribe(operation.Subscription),
        _ => null
    };
    public static uint ParseCount(IReadOnlyList<LinoLink> document) => LinksOperation<uint>.ParseCount(document);
    public static IReadOnlyList<Link<uint>> ParseLinks(IReadOnlyList<LinoLink> document) => LinksOperation<uint>.ParseLinks(document);
    public static IReadOnlyList<(Link<uint> Before, Link<uint> After)> ParseChanges(IReadOnlyList<LinoLink> document) => LinksOperation<uint>.ParseChanges(document);
    public static string? ParseName(IReadOnlyList<LinoLink> document) => LinksOperation<uint>.ParseName(document);
    public static uint? ParseLinkReply(IReadOnlyList<LinoLink> document) => LinksOperation<uint>.ParseLinkReply(document);
    public static IReadOnlyList<LinoLink> ChangesDocument(IEnumerable<(Link<uint> Before, Link<uint> After)> changes) => LinksOperation<uint>.ChangesDocument(changes);
    public static LinoLink LinkLino(Link<uint> link) => LinksOperation<uint>.LinkLino(link);
    internal static LinoLink ChangeLino(Link<uint> before, Link<uint> after, Func<Link<uint>, LinoLink> linkLino) => LinksOperation<uint>.ChangeLino(before, after, linkLino);
    internal static LinoLink Number(uint value) => LinksOperation<uint>.Number(value);
    internal static LinoLink Group(IEnumerable<LinoLink> values) => LinksOperation<uint>.Group(values);
    /// <summary>The number of a <c>(subscription: N)</c> reply.</summary>
    public static uint ParseSubscription(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document is [{ Id: "subscription", Values: [var subscription] }]
            ? ParseNumber(subscription)
            : throw MalformedReply("subscription", document);
    }

    /// <summary>The reply to a <see cref="Subscribe"/>: <c>(subscription: N)</c>.</summary>
    public static IReadOnlyList<LinoLink> SubscriptionDocument(uint subscription) =>
        new[] { Named("subscription", Number(subscription)) };

    /// <summary>
    /// Whether <paramref name="document"/> is a push rather than a reply: its first link is <c>(notify: N)</c>.
    /// </summary>
    /// <remarks>
    /// No reply starts that way: the replies to operations name their first link after the operation's result or
    /// leave it unnamed, and the links a server lists are <c>(index: source target)</c>, with two values.
    /// </remarks>
    public static bool IsNotification(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document is [{ Id: "notify", Values: [{ Values: null, Id: { } number }] }, ..]
            && uint.TryParse(number, NumberStyles.None, CultureInfo.InvariantCulture, out _);
    }

    /// <summary>
    /// A push: <c>(notify: N)</c> followed by one <c>(before) (after)</c> line per change, with plain numbers.
    /// </summary>
    public static IReadOnlyList<LinoLink> NotificationDocument(uint subscription, IEnumerable<(DoubletLink Before, DoubletLink After)> changes) =>
        ChangesDocument(changes).Prepend(Named("notify", Number(subscription))).ToList();

    /// <summary>The subscription and the changes of a <see cref="NotificationDocument"/>.</summary>
    public static LinksNotification ParseNotification(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return IsNotification(document) && document[0].Values is [var subscription]
            ? new LinksNotification(ParseNumber(subscription), ParseChanges(document.Skip(1).ToList()))
            : throw MalformedReply("notify", document);
    }

    internal static LinoLink Named(string name, params LinoLink[] values) => LinoFormat.Link(name, values.ToList());

    internal static LinoLink Numbers(params uint[] numbers) => Group(numbers.Select(Number));

    internal static LinoLink RestrictionLino(LinksRestriction restriction) => Group(restriction.Parts.Select(LinksRestriction.PartLino));

    internal static LinksRestriction ParseRestriction(LinoLink argument, int maxParts = 3)
    {
        var parts = Parts(argument);
        if (parts.Count > maxParts)
        {
            throw MalformedArgument("restriction", argument);
        }
        return new LinksRestriction(parts
            .Select(part => part is { Values: null, Id: LinksRestriction.AnyPart } ? null : (uint?)ParseNumber(part))
            .ToList());
    }

    internal static uint[] ParseNumbers(LinoLink argument, int count)
    {
        var numbers = Parts(argument).Select(ParseNumber).ToArray();
        return numbers.Length == count ? numbers : throw MalformedArgument($"{count} numbers", argument);
    }

    /// <summary>
    /// The values of an unnamed group; a lone reference is a group of one, since the canonical document model
    /// unwraps <c>(x)</c> to <c>x</c>.
    /// </summary>
    internal static IReadOnlyList<LinoLink> Parts(LinoLink argument) =>
        argument is { Id: null, Values: { } values } ? values.ToList() : new[] { argument };

    internal static uint ParseNumber(LinoLink value) => value is { Values: null, Id: { } text }
        ? ParseNumber(text)
        : throw LinoProtocolException.Malformed($"expected a number, found {LinoFormat.FormatLink(value)}");

    internal static uint ParseNumber(string text) =>
        uint.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out var number)
            ? number
            : throw LinoProtocolException.Malformed($"expected a number, found '{text}'");

    internal static InvalidOperationException NeedsServer() =>
        new("subscriptions belong to a connection to a links server");

    internal static LinoProtocolException MalformedArgument(string expected, LinoLink argument) =>
        LinoProtocolException.Malformed($"expected {expected}, found {LinoFormat.FormatLink(argument)}");

    internal static LinoProtocolException MalformedReply(string expected, IReadOnlyList<LinoLink> document) =>
        LinoProtocolException.Malformed($"expected a {expected} reply, found {LinoFormat.FormatDocument(document)}");
}

/// <summary>
/// A change a <see cref="LinksServer"/> pushed to a subscriber: the subscription it fits and its changes, a missing side
/// being the null link.
/// </summary>
/// <remarks>
/// Each side is the link as the subscription sees it: a side that does not fit the restriction (or that the subscriber
/// may not read) is the null link, so a link that leaves the subscription arrives as a deletion and one that comes
/// into it as a creation.
/// </remarks>
public sealed record LinksNotification(uint Subscription, IReadOnlyList<(DoubletLink Before, DoubletLink After)> Changes)
{
    /// <inheritdoc/>
    public bool Equals(LinksNotification? other) =>
        other is not null && Subscription == other.Subscription && Changes.SequenceEqual(other.Changes);

    /// <inheritdoc/>
    public override int GetHashCode() => HashCode.Combine(Subscription, Changes.Count);
}
