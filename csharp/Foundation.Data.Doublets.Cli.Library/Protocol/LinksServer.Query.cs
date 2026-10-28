using System.Numerics;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data.Doublets;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

public sealed partial class LinksServer
{
    /// <summary>An opt-in envelope preserving original query reference kinds across every codec.</summary>
    public static IReadOnlyList<LinoLink> QueryDocument(string query)
    {
        ArgumentNullException.ThrowIfNull(query);
        return [LinoFormat.Link("query", [LinoFormat.Reference(query)])];
    }

    internal static IReadOnlyList<LinoLink> ParseDocument(string text, QueryLimits? limits)
    {
        if (limits is null) return LinoFormat.ParseDocument(text);
        try
        {
            limits.Input(text);
            var original = new Link.Foundation.Links.Notation.Parser(true, limits.MaxDepth).Parse(text);
            limits.Nodes(original);
            var document = LinoFormat.ParseDocument(text);
            limits.Document(document);
            return document;
        }
        catch (Link.Foundation.Links.Notation.ParseException error)
        {
            throw new LinoProtocolException(LinoProtocolErrorKind.InvalidLino, error.Message, error);
        }
        catch (InvalidOperationException error)
        {
            throw new LinoProtocolException(LinoProtocolErrorKind.LimitExceeded, error.Message, error);
        }
    }

    /// <summary>Executes the same standard operations and query protocol with selected unsigned addresses.</summary>
    public static IReadOnlyList<LinoLink> ExecuteRequest<T>(
        INamedTypesLinks<T> links, IReadOnlyList<LinoLink> document, bool autoCreateMissingReferences = false)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
        => ExecuteRequestCore(links, document, autoCreateMissingReferences, null);

    private static IReadOnlyList<LinoLink> ExecuteRequestCore<T>(INamedTypesLinks<T> links,
        IReadOnlyList<LinoLink> document, bool autoCreateMissingReferences, Action<string>? trace)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        ArgumentNullException.ThrowIfNull(links);
        ArgumentNullException.ThrowIfNull(document);
        try
        {
            var limits = links.QueryLimits;
            limits?.Document(document);
            if (trace is not null) trace($"request: {LinoFormat.FormatDocument(document)}");
            string? original = null;
            if (document is [{ Id: "query" }])
            {
                if (document[0].Values is not [{ Values: null, Id: { } text }])
                    throw new ArgumentException("Expected (query: original-text).");
                original = text;
            }
            if (original is null && LinksOperation<T>.FromDocument(document) is { } operation)
                return operation.Execute(links);
            IReadOnlyList<LinoLink> reply;
            if (document.Count == 0 || original is not null && string.IsNullOrWhiteSpace(original))
            {
                var any = links.Constants.Any;
                var rows = links.All(new LinkParts<T>(any, any, any)).Select(Owned<T>).OrderBy(row => row.Index).ToList();
                limits?.Results(rows.Count);
                reply = rows.Select(row => LinkLino(links, row)).ToList();
            }
            else
            {
                var changes = new List<(Link<T> Before, Link<T> After)>();
                AdvancedMixedQueryProcessor<T>.ProcessQuery(links, new()
                {
                    Query = original ?? LinoFormat.FormatDocument(document),
                    AutoCreateMissingReferences = autoCreateMissingReferences,
                    ChangesHandler = (before, after) =>
                    {
                        changes.Add((Owned<T>(before), Owned<T>(after)));
                        return links.Constants.Continue;
                    }
                });
                reply = ChangesSimplifier.SimplifyChanges(changes)
                    .Select(change => LinksOperation<T>.ChangeLino(change.Before, change.After, row => LinkLino(links, row))).ToList();
            }
            limits?.DecodedReply(reply);
            return reply;
        }
        catch (Exception error) when (error is not OutOfMemoryException)
        {
            return ErrorDocument(error.Message);
        }
    }

    private static Link<T> Owned<T>(IList<T>? row) where T : IUnsignedNumber<T>
        => row is null ? default : new(row[0], row[1], row[2]);

    private static LinoLink LinkLino<T>(INamedTypesLinks<T> links, Link<T> row)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
        => LinoFormat.Link(ReferenceName(links, row.Index),
            [LinoFormat.Reference(ReferenceName(links, row.Source)), LinoFormat.Reference(ReferenceName(links, row.Target))]);

    private static string ReferenceName<T>(INamedTypesLinks<T> links, T id)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
        => links.GetName(id) ?? id.ToString(null, System.Globalization.CultureInfo.InvariantCulture);
}
