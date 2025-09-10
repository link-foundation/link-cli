using Foundation.Data.Doublets.Cli.Benchmarks.Models;
using Platform.Protocols.Lino;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Benchmarks.Serialization;

public static class LinoSerializer
{
    private static readonly Parser Parser = new();

    public static string SerializeLinkData(LinkData link)
    {
        return $"({link.Id}: {link.Source} {link.Target})";
    }

    public static string SerializeLinkDataCollection(IEnumerable<LinkData> links)
    {
        var linkStrings = links.Select(SerializeLinkData);
        return $"({string.Join(" ", linkStrings)})";
    }

    public static string SerializeCreateRequest(CreateLinkRequest request)
    {
        return $"() (({request.Source} {request.Target}))";
    }

    public static string SerializeUpdateRequest(UpdateLinkRequest request)
    {
        return $"(({request.Id}: * *)) (({request.Id}: {request.Source} {request.Target}))";
    }

    public static string SerializeDeleteRequest(DeleteLinkRequest request)
    {
        return $"(({request.Id}: * *)) ()";
    }

    public static string SerializeQueryRequest(QueryLinksRequest request)
    {
        var id = request.Id?.ToString() ?? "*";
        var source = request.Source?.ToString() ?? "*";
        var target = request.Target?.ToString() ?? "*";
        
        return $"(({id}: {source} {target})) (({id}: {source} {target}))";
    }

    public static LinkData? DeserializeLinkData(string linoString)
    {
        try
        {
            var elements = Parser.Parse(linoString);
            if (elements.Count == 0) return null;

            var element = elements[0];
            if (element.Values?.Count != 2)
                return null;

            var id = element.Id != null && uint.TryParse(element.Id, out uint parsedId) ? parsedId : 0;
            var source = element.Values[0].Id != null && uint.TryParse(element.Values[0].Id, out uint parsedSource) ? parsedSource : 0;
            var target = element.Values[1].Id != null && uint.TryParse(element.Values[1].Id, out uint parsedTarget) ? parsedTarget : 0;

            return new LinkData { Id = id, Source = source, Target = target };
        }
        catch
        {
            return null;
        }
    }

    public static IEnumerable<LinkData> DeserializeLinkDataCollection(string linoString)
    {
        try
        {
            var elements = Parser.Parse(linoString);
            var results = new List<LinkData>();

            foreach (var element in elements)
            {
                if (element.Values?.Count == 2)
                {
                    var id = element.Id != null && uint.TryParse(element.Id, out uint parsedId) ? parsedId : 0;
                    var source = element.Values[0].Id != null && uint.TryParse(element.Values[0].Id, out uint parsedSource) ? parsedSource : 0;
                    var target = element.Values[1].Id != null && uint.TryParse(element.Values[1].Id, out uint parsedTarget) ? parsedTarget : 0;

                    results.Add(new LinkData { Id = id, Source = source, Target = target });
                }
            }

            return results;
        }
        catch
        {
            return [];
        }
    }
}