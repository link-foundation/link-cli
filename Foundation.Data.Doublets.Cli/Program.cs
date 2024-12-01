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
  if (!string.IsNullOrWhiteSpace(query))
  {
    ProcessQuery(links, query);
  }
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

static DoubletLink ToDoubletLink(ILinks<uint> links, LinoLink linoLink, uint defaultValue)
{
  uint index = defaultValue;
  uint source = defaultValue;
  uint target = defaultValue;
  if (!string.IsNullOrEmpty(linoLink.Id) && uint.TryParse(linoLink.Id, out uint linkId))
  {
    index = linkId;
  }
  if (linoLink.Values?.Count == 2)
  {
    var sourceLink = linoLink.Values[0];
    var targetLink = linoLink.Values[1];
    if (!string.IsNullOrEmpty(sourceLink.Id) && uint.TryParse(sourceLink.Id, out uint sourceId))
    {
      source = sourceId;
    }
    if (!string.IsNullOrEmpty(targetLink.Id) && uint.TryParse(targetLink.Id, out uint targetId))
    {
      target = targetId;
    }
  }
  return new DoubletLink(index, source, target);
}

static void ProcessQuery(ILinks<uint> links, string query)
{
  var parser = new Parser();
  var parsedLinks = parser.Parse(query);

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
  var any = links.Constants.Any;

  if (outerLinkValues == null)
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
  else if ((restrictionLink.Values?.Count > 0) &&
           (substitutionLink.Values?.Count > 0))
  {
    // Update operation (only single link is supported at the moment)
    var restrictionDoublet = ToDoubletLink(links, restrictionLink.Values[0], any);
    var substitutionDoublet = ToDoubletLink(links, substitutionLink.Values[0], @null);

    links.Update(restrictionDoublet, substitutionDoublet, (before, after) =>
    {
      return links.Constants.Continue;
    });

    return;
  }
  else if (substitutionLink.Values?.Count == 0) // If substitution is empty, perform delete operation
  {
    foreach (var linkToDelete in restrictionLink.Values ?? [])
    {
      var queryLink = ToDoubletLink(links, linkToDelete, any);
      links.DeleteByQuery(queryLink);
    }
    return;
  }
  else if (restrictionLink.Values?.Count == 0) // If restriction is empty, perform create operation
  {
    foreach (var linkToCreate in substitutionLink.Values ?? [])
    {
      var doubletLink = ToDoubletLink(links, linkToCreate, @null);
      links.GetOrCreate(doubletLink.Source, doubletLink.Target);
    }
    return;
  }
}