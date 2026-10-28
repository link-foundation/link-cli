using Platform.Data.Doublets;
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
}
