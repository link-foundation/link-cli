using System.CommandLine;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;

using DoubletLink = Platform.Data.Doublets.Link<uint>;
using MixedQueryProcessor = Foundation.Data.Doublets.Cli.MixedQueryProcessor;

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
    // ProcessQuery(links, query);
    MixedQueryProcessor.ProcessQuery(links, query);
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
