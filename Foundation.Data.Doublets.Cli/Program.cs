using System.CommandLine;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;

using DoubletLink = Platform.Data.Doublets.Link<uint>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

var dbOption = new Option<string>(
  name: "--db",
  description: "Path to the links database file",
  getDefaultValue: () => "db.links"
);
dbOption.AddAlias("-d");

var queryOption = new Option<string>(
  name: "--query",
  description: "LiNo query for CRUD operation"
);
queryOption.AddAlias("-q");

var queryArgument = new Argument<string>(
  name: "query",
  description: "LiNo query for CRUD operation"
);
queryArgument.Arity = ArgumentArity.ZeroOrOne;

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
  dbOption,
  queryOption,
  queryArgument
};

rootCommand.SetHandler((string db, string queryOptionValue, string queryArgumentValue) =>
{
  using var links = new UnitedMemoryLinks<uint>(db);

  var decoratedLinks = links.DecorateWithAutomaticUniquenessAndUsagesResolution();

  var effectiveQuery = !string.IsNullOrWhiteSpace(queryOptionValue) ? queryOptionValue : queryArgumentValue;

  if (!string.IsNullOrWhiteSpace(effectiveQuery))
  {
    var options = new QueryProcessor.Options
    {
      Query = effectiveQuery,
      ChangesHandler = (before, after) =>
      {
        var beforeLink = new DoubletLink(before);
        var afterLink = new DoubletLink(after);
        Console.WriteLine($"{links.Format(beforeLink)} ↦ {links.Format(afterLink)}");
        return links.Constants.Continue;
      }
    };

    QueryProcessor.ProcessQuery(decoratedLinks, options);
  }
  PrintAllLinks(decoratedLinks);
}, dbOption, queryOption, queryArgument);

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
