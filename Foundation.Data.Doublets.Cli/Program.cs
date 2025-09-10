using System.CommandLine;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;

using static Foundation.Data.Doublets.Cli.ChangesSimplifier;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;
using Foundation.Data.Doublets.Cli;
using System.Text.RegularExpressions;

const string defaultDatabaseFilename = "db.links";

var dbOption = new Option<string>(
  name: "--db",
  description: "Path to the links database file",
  getDefaultValue: () => defaultDatabaseFilename
);
dbOption.AddAlias("--data-source");
dbOption.AddAlias("--data");
dbOption.AddAlias("-d");

var queryOption = new Option<string>(
  name: "--query",
  description: "LiNo query for CRUD operation"
);
queryOption.AddAlias("--apply");
queryOption.AddAlias("--do");
queryOption.AddAlias("-q");

var queryArgument = new Argument<string>(
  name: "query",
  description: "LiNo query for CRUD operation"
);
queryArgument.Arity = ArgumentArity.ZeroOrOne;

var traceOption = new Option<bool>(
  name: "--trace",
  description: "Enable trace (verbose output)",
  getDefaultValue: () => false
);
traceOption.AddAlias("-t");

var structureOption = new Option<uint?>(
  name: "--structure",
  description: "ID of the link to format its structure"
);
structureOption.AddAlias("-s");

var beforeOption = new Option<bool>(
  name: "--before",
  description: "Print the state of the database before applying changes",
  getDefaultValue: () => false
);
beforeOption.AddAlias("-b");

var changesOption = new Option<bool>(
  name: "--changes",
  description: "Print the changes applied by the query",
  getDefaultValue: () => false
);
changesOption.AddAlias("-c");

var afterOption = new Option<bool>(
  name: "--after",
  description: "Print the state of the database after applying changes",
  getDefaultValue: () => false
);
afterOption.AddAlias("--links");
afterOption.AddAlias("-a");

// Server command options
var portOption = new Option<int>(
  name: "--port",
  description: "Port number for the GraphQL server",
  getDefaultValue: () => 5000
);
portOption.AddAlias("-p");

// Create server command
var serverCommand = new Command("serve", "Start LINO GraphQL API server")
{
  dbOption,
  portOption,
  traceOption
};

// Create query command (existing functionality)
var queryCommand = new Command("query", "Execute LINO query")
{
  dbOption,
  queryOption,
  queryArgument,
  traceOption,
  structureOption,
  beforeOption,
  changesOption,
  afterOption
};

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
  serverCommand,
  queryCommand,
  // Keep backward compatibility - add options directly to root too
  dbOption,
  queryOption,
  queryArgument,
  traceOption,
  structureOption,
  beforeOption,
  changesOption,
  afterOption
};

// Server command handler
serverCommand.SetHandler(
  async (string db, int port, bool trace) =>
  {
    var decoratedLinks = new NamedLinksDecorator<uint>(db, trace);
    var server = new LinoGraphQLServer(decoratedLinks, port, trace);
    
    Console.WriteLine($"Starting LINO GraphQL API server on port {port}...");
    Console.WriteLine("Press Ctrl+C to stop the server");
    
    try
    {
      await server.StartAsync();
    }
    catch (Exception ex)
    {
      Console.Error.WriteLine($"Server error: {ex.Message}");
      Environment.Exit(1);
    }
  },
  dbOption, portOption, traceOption
);

// Query command handler
queryCommand.SetHandler(
  (string db, string queryOptionValue, string queryArgumentValue, bool trace, uint? structure, bool before, bool changes, bool after) =>
  {
    ExecuteQueryCommand(db, queryOptionValue, queryArgumentValue, trace, structure, before, changes, after);
  },
  dbOption, queryOption, queryArgument, traceOption, structureOption, beforeOption, changesOption, afterOption
);

// Root command handler (backward compatibility)
rootCommand.SetHandler(
  (string db, string queryOptionValue, string queryArgumentValue, bool trace, uint? structure, bool before, bool changes, bool after) =>
  {
    ExecuteQueryCommand(db, queryOptionValue, queryArgumentValue, trace, structure, before, changes, after);
  },
  dbOption, queryOption, queryArgument, traceOption, structureOption, beforeOption, changesOption, afterOption
);

static void ExecuteQueryCommand(string db, string queryOptionValue, string queryArgumentValue, bool trace, uint? structure, bool before, bool changes, bool after)
{
  var decoratedLinks = new NamedLinksDecorator<uint>(db, trace);

  // If --structure is provided, handle it separately
  if (structure.HasValue)
  {
    var linkId = structure.Value;
    try
    {
      var structureFormatted = decoratedLinks.FormatStructure(linkId, link => decoratedLinks.IsFullPoint(linkId), true, true);
      Console.WriteLine(Namify(decoratedLinks, structureFormatted));
    }
    catch (Exception ex)
    {
      Console.Error.WriteLine($"Error formatting structure for link ID {linkId}: {ex.Message}");
      Environment.Exit(1);
    }
    return; // Exit after handling --structure
  }

  if (before)
  {
    PrintAllLinks(decoratedLinks);
  }

  var effectiveQuery = !string.IsNullOrWhiteSpace(queryOptionValue) ? queryOptionValue : queryArgumentValue;

  var changesList = new List<(DoubletLink Before, DoubletLink After)>();

  if (!string.IsNullOrWhiteSpace(effectiveQuery))
  {
    var options = new QueryProcessor.Options
    {
      Query = effectiveQuery,
      Trace = trace,
      ChangesHandler = (beforeLink, afterLink) =>
      {
        changesList.Add((new DoubletLink(beforeLink), new DoubletLink(afterLink)));
        return decoratedLinks.Constants.Continue;
      }
    };

    QueryProcessor.ProcessQuery(decoratedLinks, options);
  }

  if (changes && changesList.Any())
  {
    // Simplify the collected changes
    var simplifiedChanges = SimplifyChanges(changesList);

    // Print the simplified changes
    foreach (var (linkBefore, linkAfter) in simplifiedChanges)
    {
      PrintChange(decoratedLinks, linkBefore, linkAfter);
    }
  }

  if (after)
  {
    PrintAllLinks(decoratedLinks);
  }
}

await rootCommand.InvokeAsync(args);

static string Namify(NamedLinksDecorator<uint> namedLinks, string linksNotation)
{
  var numberGlobalRegex = new Regex(@"\d+");
  var matches = numberGlobalRegex.Matches(linksNotation);
  var newLinksNotation = linksNotation;
  foreach (Match match in matches)
  {
    var number = match.Value;
    var numberLink = uint.Parse(number);
    var name = namedLinks.GetName(numberLink);
    if (name != null)
    {
      newLinksNotation = newLinksNotation.Replace(number, name);
    }
  }
  return newLinksNotation;
}

static void PrintAllLinks(NamedLinksDecorator<uint> links)
{
  var any = links.Constants.Any;
  var query = new DoubletLink(index: any, source: any, target: any);

  links.Each(query, link =>
  {
    var formattedLink = links.Format(link);
    Console.WriteLine(Namify(links, formattedLink));
    return links.Constants.Continue;
  });
}

static void PrintChange(NamedLinksDecorator<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
{
  var beforeText = linkBefore.IsNull() ? "" : links.Format(linkBefore);
  var afterText = linkAfter.IsNull() ? "" : links.Format(linkAfter);
  var formattedChange = $"({beforeText}) ({afterText})";
  Console.WriteLine(Namify(links, formattedChange));
}