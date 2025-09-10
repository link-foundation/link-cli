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

// Create benchmark subcommand
var benchmarkCommand = new Command("benchmark", "Run benchmark comparing CLI access vs LiNo protocol server access");

var iterationsOption = new Option<int>(
  name: "--iterations",
  description: "Number of iterations per query",
  getDefaultValue: () => 10
);
iterationsOption.AddAlias("-i");

var warmupOption = new Option<int>(
  name: "--warmup",
  description: "Number of warmup iterations",
  getDefaultValue: () => 3
);
warmupOption.AddAlias("-w");

var serverPortOption = new Option<int>(
  name: "--server-port",
  description: "Port for the benchmark server",
  getDefaultValue: () => 8080
);
serverPortOption.AddAlias("-p");

var testQueriesOption = new Option<string[]>(
  name: "--queries",
  description: "Test queries to benchmark (if not specified, uses default set)"
);
testQueriesOption.AddAlias("-q");

benchmarkCommand.AddOption(dbOption);
benchmarkCommand.AddOption(traceOption);
benchmarkCommand.AddOption(iterationsOption);
benchmarkCommand.AddOption(warmupOption);
benchmarkCommand.AddOption(serverPortOption);
benchmarkCommand.AddOption(testQueriesOption);

benchmarkCommand.SetHandler(async (string db, bool trace, int iterations, int warmup, int serverPort, string[] testQueries) =>
{
  var defaultQueries = new[]
  {
    "() ((1 1))",                                                    // Create single link
    "() ((1 1) (2 2))",                                             // Create multiple links
    "((($i: $s $t)) (($i: $s $t)))",                               // Read all links
    "((1: 1 1)) ((1: 1 2))",                                       // Update single link
    "((1 2)) ()"                                                   // Delete link (will only work if link exists)
  };

  var queries = testQueries?.Any() == true ? testQueries.ToList() : defaultQueries.ToList();

  var benchmarkRunner = new BenchmarkRunner(db, trace);
  var options = new BenchmarkOptions
  {
    TestQueries = queries,
    IterationsPerQuery = iterations,
    WarmupIterations = warmup,
    ServerPort = serverPort
  };

  try
  {
    var results = await benchmarkRunner.RunBenchmarkAsync(options);
    results.PrintReport();
  }
  catch (Exception ex)
  {
    Console.Error.WriteLine($"Benchmark failed: {ex.Message}");
    Environment.Exit(1);
  }
}, dbOption, traceOption, iterationsOption, warmupOption, serverPortOption, testQueriesOption);

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
  dbOption,
  queryOption,
  queryArgument,
  traceOption,
  structureOption,
  beforeOption,
  changesOption,
  afterOption,
  benchmarkCommand
};

rootCommand.SetHandler(
  (string db, string queryOptionValue, string queryArgumentValue, bool trace, uint? structure, bool before, bool changes, bool after) =>
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
  },
  // Explicitly specify the type parameters
  dbOption, queryOption, queryArgument, traceOption, structureOption, beforeOption, changesOption, afterOption
);

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