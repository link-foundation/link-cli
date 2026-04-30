using System.CommandLine;
using System.CommandLine.Invocation;
using Foundation.Data.Doublets.Cli;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Protocols.Lino;

using static Foundation.Data.Doublets.Cli.ChangesSimplifier;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

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

var autoCreateMissingReferencesOption = new Option<bool>(
  name: "--auto-create-missing-references",
  description: "Create missing numeric and named references as self-referential point links",
  getDefaultValue: () => false
);

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

var outputOption = new Option<string?>(
  name: "--out",
  description: "Path to write the complete database as a LiNo file"
);
outputOption.AddAlias("--lino-output");

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
  dbOption,
  queryOption,
  queryArgument,
  traceOption,
  autoCreateMissingReferencesOption,
  structureOption,
  beforeOption,
  changesOption,
  afterOption,
  outputOption
};

rootCommand.SetHandler(
  (InvocationContext context) =>
  {
    var db = context.ParseResult.GetValueForOption(dbOption)!;
    var queryOptionValue = context.ParseResult.GetValueForOption(queryOption) ?? "";
    var queryArgumentValue = context.ParseResult.GetValueForArgument(queryArgument) ?? "";
    var trace = context.ParseResult.GetValueForOption(traceOption);
    var autoCreateMissingReferences = context.ParseResult.GetValueForOption(autoCreateMissingReferencesOption);
    var structure = context.ParseResult.GetValueForOption(structureOption);
    var before = context.ParseResult.GetValueForOption(beforeOption);
    var changes = context.ParseResult.GetValueForOption(changesOption);
    var after = context.ParseResult.GetValueForOption(afterOption);
    var outputPath = context.ParseResult.GetValueForOption(outputOption);

    var decoratedLinks = new NamedLinksDecorator<uint>(db, trace);

    if (structure.HasValue)
    {
      var linkId = structure.Value;
      try
      {
        var structureFormatted = decoratedLinks.FormatStructure(linkId, link => decoratedLinks.IsFullPoint(linkId), true, true);
        Console.WriteLine(LinoDatabaseOutput.Namify(decoratedLinks, structureFormatted));
      }
      catch (Exception ex)
      {
        Console.Error.WriteLine($"Error formatting structure for link ID {linkId}: {ex.Message}");
        context.ExitCode = 1;
        return;
      }

      TryWriteLinoOutput(decoratedLinks, outputPath, context);
      return;
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
        AutoCreateMissingReferences = autoCreateMissingReferences,
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
      if (trace)
      {
        Console.WriteLine("[DEBUG] Raw changes before simplification:");
        for (int i = 0; i < changesList.Count; i++)
        {
          var (beforeLink, afterLink) = changesList[i];
          Console.WriteLine($"[DEBUG] {i + 1}. ({beforeLink.Index}: {beforeLink.Source} {beforeLink.Target}) -> ({afterLink.Index}: {afterLink.Source} {afterLink.Target})");
        }
        Console.WriteLine($"[DEBUG] Total raw changes: {changesList.Count}");
      }

      var simplifiedChanges = SimplifyChanges(changesList);

      if (trace)
      {
        Console.WriteLine($"[DEBUG] Simplified changes count: {simplifiedChanges.Count()}");
      }

      foreach (var (linkBefore, linkAfter) in simplifiedChanges)
      {
        PrintChange(decoratedLinks, linkBefore, linkAfter);
      }
    }

    if (after)
    {
      PrintAllLinks(decoratedLinks);
    }

    TryWriteLinoOutput(decoratedLinks, outputPath, context);
  }
);

await rootCommand.InvokeAsync(args);

static void PrintAllLinks(NamedLinksDecorator<uint> links)
{
  LinoDatabaseOutput.WriteDatabase(links, Console.Out);
}

static void PrintChange(NamedLinksDecorator<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
{
  Console.WriteLine(LinoDatabaseOutput.FormatChange(links, linkBefore, linkAfter));
}

static bool TryWriteLinoOutput(NamedLinksDecorator<uint> links, string? outputPath, InvocationContext context)
{
  if (string.IsNullOrWhiteSpace(outputPath))
  {
    return true;
  }

  try
  {
    LinoDatabaseOutput.WriteToFile(links, outputPath);
    return true;
  }
  catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException || ex is NotSupportedException)
  {
    Console.Error.WriteLine($"Error writing LiNo output file '{outputPath}': {ex.Message}");
    context.ExitCode = 1;
    return false;
  }
}
