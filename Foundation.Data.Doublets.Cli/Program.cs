using System;
using System.CommandLine;
using System.CommandLine.Invocation;
using System.Collections.Generic;
using System.Linq;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;

using static Foundation.Data.Doublets.Cli.ChangesSimplifier;

using DoubletLink = Platform.Data.Doublets.Link<uint>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;
using Platform.Data;

var dbOption = new Option<string>(
  name: "--db",
  description: "Path to the links database file",
  getDefaultValue: () => "db.links"
);
dbOption.AddAlias("--data-source");
dbOption.AddAlias("-d");

var queryOption = new Option<string>(
  name: "--query",
  description: "LiNo query for CRUD operation"
);
queryOption.AddAlias("--apply");
queryOption.AddAlias("-a");
queryOption.AddAlias("-q");

var queryArgument = new Argument<string>(
  name: "query",
  description: "LiNo query for CRUD operation"
);
queryArgument.Arity = ArgumentArity.ZeroOrOne;

// New option for enabling trace
var traceOption = new Option<bool>(
  name: "--trace",
  description: "Enable trace (verbose output)",
  getDefaultValue: () => false
);
traceOption.AddAlias("-t");

// New option for structure
var structureOption = new Option<uint?>(
  name: "--structure",
  description: "ID of the link to format its structure"
);
structureOption.AddAlias("-s");

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
  dbOption,
  queryOption,
  queryArgument,
  traceOption,
  structureOption // Add the new structure option to the root command
};

// Specify the generic type arguments explicitly
rootCommand.SetHandler(
  (string db, string queryOptionValue, string queryArgumentValue, bool trace, uint? structure) =>
  {
    using var links = new UnitedMemoryLinks<uint>(db);
    var decoratedLinks = links.DecorateWithAutomaticUniquenessAndUsagesResolution();

    // If --structure is provided, handle it separately
    if (structure.HasValue)
    {
      var linkId = structure.Value;
      try
      {
        var structureFormatted = decoratedLinks.FormatStructure(linkId, link => decoratedLinks.IsFullPoint(linkId));
        Console.WriteLine(structureFormatted);
      }
      catch (Exception ex)
      {
        Console.Error.WriteLine($"Error formatting structure for link ID {linkId}: {ex.Message}");
        Environment.Exit(1);
      }
      return; // Exit after handling --structure
    }

    var effectiveQuery = !string.IsNullOrWhiteSpace(queryOptionValue) ? queryOptionValue : queryArgumentValue;

    var changes = new List<(DoubletLink Before, DoubletLink After)>();

    if (!string.IsNullOrWhiteSpace(effectiveQuery))
    {
      var options = new QueryProcessor.Options
      {
        Query = effectiveQuery,
        Trace = trace, // Pass the trace flag here
        ChangesHandler = (before, after) =>
        {
          // Collect changes instead of printing immediately
          changes.Add((new DoubletLink(before), new DoubletLink(after)));
          return links.Constants.Continue;
        }
      };

      QueryProcessor.ProcessQuery(decoratedLinks, options);
    }

    if (changes.Any())
    {
      // Simplify the collected changes
      var simplifiedChanges = SimplifyChanges(changes);

      // Print the simplified changes
      foreach (var (before, after) in simplifiedChanges)
      {
        Console.WriteLine($"{links.Format(before)} ↦ {links.Format(after)}");
      }
    }

    PrintAllLinks(decoratedLinks);
  },
  // Explicitly specify the type parameters
  dbOption, queryOption, queryArgument, traceOption, structureOption
);

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