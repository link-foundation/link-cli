using System.CommandLine;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

var dbOption = new Option<string>(
  name: "--db",
  description: "Path to the links database file",
  getDefaultValue: () => "db.links"
);

var queryOption = new Option<string>(
  name: "--query",
  description: "LiNo query for CRUD operation"
);

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
  dbOption,
  queryOption
};

rootCommand.SetHandler((string db, string query) =>
{
  using var links = new UnitedMemoryLinks<uint>(db);

  if (string.IsNullOrWhiteSpace(query))
  {
    PrintAllLinks(links);
    return;
  }

  var parser = new Parser();
  var parsedLinks = parser.Parse(query);

  // Process parsed links based on CRUD operations
  ProcessLinks(links, parsedLinks);

  PrintAllLinks(links);

}, dbOption, queryOption);

await rootCommand.InvokeAsync(args);


static void PrintAllLinks(ILinks<uint> links)
{
  var any = links.Constants.Any;
  var query = new DoubletLink(index: any, source: any, target: any);

  links.Each(query, link =>
  {
    Console.WriteLine(links.Format(link));
    return links.Constants.Continue;
  });
}

static DoubletLink BuildQueryFromLinoLink(ILinks<uint> links, LinoLink linoLink)
{
  uint index = links.Constants.Any;
  uint source = links.Constants.Any;
  uint target = links.Constants.Any;

  // Console.WriteLine($"Building query from LinoLink: {linoLink}");

  if (!string.IsNullOrEmpty(linoLink.Id) && uint.TryParse(linoLink.Id, out uint linkId))
  {
    index = linkId;
  }

  var restrictionLink = linoLink.Values[0];

  if (restrictionLink.Values?.Count >= 2)
  {
    source = GetLinkAddress(links, restrictionLink.Values[0]);
    target = GetLinkAddress(links, restrictionLink.Values[1]);
  }

  return new DoubletLink(index, source, target);
}

static uint GetLinkAddress(ILinks<uint> links, LinoLink? link)
{
  if (link == null)
  {
    return links.Constants.Null;
  }

  LinoLink nonNullLink = (LinoLink)link;
  if (!string.IsNullOrEmpty(nonNullLink.Id) && uint.TryParse(nonNullLink.Id, out uint linkId))
  {
    // Console.WriteLine($"Link ID: {linkId}");
    return linkId;
  }

  if (nonNullLink.Values?.Count >= 2)
  {
    uint source = GetLinkAddress(links, nonNullLink.Values[0]);
    uint target = GetLinkAddress(links, nonNullLink.Values[1]);
    // Console.WriteLine($"Source: {source}, Target: {target}");
    if (source != links.Constants.Null && target != links.Constants.Null)
    {
      return links.GetOrCreate(source, target);
    }
  }
  else if (nonNullLink.Values?.Count == 1)
  {
    // Console.WriteLine($"Link Value: {nonNullLink.Values[0]}");
    return GetLinkAddress(links, nonNullLink.Values[0]);
  }
  return links.Constants.Null;
}

static void ProcessLinks(ILinks<uint> links, IList<LinoLink> parsedLinks)
{
  if (parsedLinks.Count == 0)
  {
    return;
  }

  var outerLink = parsedLinks[0];

  var outerLinkValues = outerLink.Values;

  if (outerLinkValues?.Count < 2)
  {
    return;
  }

  var @null = links.Constants.Null;

  if (outerLinkValues == null) // To avoid warning
  {
    return;
  }

  var restrictionLink = outerLinkValues[0];
  var substitutionLink = outerLinkValues[1];

  if ((restrictionLink.Values?.Count == 0) &&
      (substitutionLink.Values?.Count == 0))
  {
    return;
  }

  // If substitution is empty, perform delete operation
  if (substitutionLink.Values?.Count == 0)
  {
    // Build query from restrictionLink
    var query = BuildQueryFromLinoLink(links, restrictionLink);

    // Console.WriteLine($"Deleting links with query: {query}");

    links.DeleteByQuery(query);
    return;
  }

  // If restriction is empty, perform create operation
  if (restrictionLink.Values?.Count == 0)
  {
    // Process substitutionLink
    if (substitutionLink.Values?.Count > 0)
    {
      foreach (var linkToCreate in substitutionLink.Values)
      {
        uint linkAddress = GetLinkAddress(links, linkToCreate);
        if (linkAddress == @null)
        {
          Console.WriteLine("Failed to create link.");
        }
      }
    }
    return;
  }

  // Existing code for updates remains unchanged
  uint linkId = @null;
  uint restrictionSource = @null;
  uint restrictionTarget = @null;
  uint substitutionSource = @null;
  uint substitutionTarget = @null;

  // Process restrictionLink
  if (restrictionLink.Values?.Count > 0)
  {
    var restrictionInnerLink = restrictionLink.Values[0];
    linkId = GetLinkAddress(links, restrictionInnerLink);

    if (linkId == @null)
    {
      return;
    }

    if (restrictionInnerLink.Values?.Count >= 2)
    {
      restrictionSource = GetLinkAddress(links, restrictionInnerLink.Values[0]);
      restrictionTarget = GetLinkAddress(links, restrictionInnerLink.Values[1]);

      if (restrictionSource == @null || restrictionTarget == @null)
      {
        return;
      }
    }
    else
    {
      return;
    }
  }
  else
  {
    return;
  }

  // Process substitutionLink
  if (substitutionLink.Values?.Count > 0)
  {
    var substitutionInnerLink = substitutionLink.Values[0];

    if (substitutionInnerLink.Values?.Count >= 2)
    {
      substitutionSource = GetLinkAddress(links, substitutionInnerLink.Values[0]);
      substitutionTarget = GetLinkAddress(links, substitutionInnerLink.Values[1]);

      if (substitutionSource == @null || substitutionTarget == @null)
      {
        return;
      }
    }
    else
    {
      return;
    }
  }
  else
  {
    return;
  }

  var restriction = new List<uint> { linkId, restrictionSource, restrictionTarget };
  var substitution = new List<uint> { linkId, substitutionSource, substitutionTarget };

  links.Update(restriction, substitution, (before, after) =>
  {
    return links.Constants.Continue;
  });
}