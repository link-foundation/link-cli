using System.CommandLine;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;
using Microsoft.AspNetCore.Hosting;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.AspNetCore.Builder;

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
  description: "Port for the REST API server",
  getDefaultValue: () => 5000
);
portOption.AddAlias("-p");

var hostOption = new Option<string>(
  name: "--host",
  description: "Host address for the REST API server",
  getDefaultValue: () => "localhost"
);
hostOption.AddAlias("-h");

// Create server command
var serverCommand = new Command("serve", "Start REST API server")
{
  dbOption,
  portOption,
  hostOption,
  traceOption
};

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
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

rootCommand.AddCommand(serverCommand);

// Server command handler
serverCommand.SetHandler(
  async (string db, int port, string host, bool trace) =>
  {
    Console.WriteLine($"Starting LINO REST API server on {host}:{port}");
    Console.WriteLine($"Database: {db}");
    Console.WriteLine($"Trace mode: {trace}");
    
    var builder = WebApplication.CreateBuilder();
    
    // Configure services
    builder.Services.AddControllers();
    builder.Services.AddEndpointsApiExplorer();
    builder.Services.AddSwaggerGen();
    
    // Configure database path
    builder.Configuration["Database:Path"] = db;
    
    // Configure web server
    builder.WebHost.UseUrls($"http://{host}:{port}");
    
    var app = builder.Build();
    
    // Configure pipeline
    if (app.Environment.IsDevelopment())
    {
        app.UseSwagger();
        app.UseSwaggerUI();
    }
    
    app.MapControllers();
    
    Console.WriteLine($"Server started! Visit http://{host}:{port}/swagger for API documentation");
    
    await app.RunAsync();
  },
  dbOption, portOption, hostOption, traceOption
);

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