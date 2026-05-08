using System.CommandLine;
using Foundation.Data.Doublets.Cli;
using Platform.Data;
using Platform.Data.Doublets;

using static Foundation.Data.Doublets.Cli.ChangesSimplifier;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

const string defaultDatabaseFilename = "db.links";

var dbOption = new Option<string>("--db", "--data-source", "--data", "-d")
{
  Description = "Path to the links database file",
  DefaultValueFactory = _ => defaultDatabaseFilename
};

var queryOption = new Option<string?>("--query", "--apply", "--do", "-q")
{
  Description = "LiNo query for CRUD operation"
};

var queryArgument = new Argument<string?>("query")
{
  Description = "LiNo query for CRUD operation",
  Arity = ArgumentArity.ZeroOrOne
};

var traceOption = new Option<bool>("--trace", "-t")
{
  Description = "Enable trace (verbose output)",
  DefaultValueFactory = _ => false
};

var autoCreateMissingReferencesOption = new Option<bool>("--auto-create-missing-references")
{
  Description = "Create missing numeric and named references as self-referential point links",
  DefaultValueFactory = _ => false
};

var structureOption = new Option<uint?>("--structure", "-s")
{
  Description = "ID of the link to format its structure"
};

var beforeOption = new Option<bool>("--before", "-b")
{
  Description = "Print the state of the database before applying changes",
  DefaultValueFactory = _ => false
};

var changesOption = new Option<bool>("--changes", "-c")
{
  Description = "Print the changes applied by the query",
  DefaultValueFactory = _ => false
};

var afterOption = new Option<bool>("--after", "--links", "-a")
{
  Description = "Print the state of the database after applying changes",
  DefaultValueFactory = _ => false
};

var outputOption = new Option<string?>("--out", "--lino-output", "--export")
{
  Description = "Path to write the complete database as a LiNo file"
};

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store");
rootCommand.Options.Add(dbOption);
rootCommand.Options.Add(queryOption);
rootCommand.Arguments.Add(queryArgument);
rootCommand.Options.Add(traceOption);
rootCommand.Options.Add(autoCreateMissingReferencesOption);
rootCommand.Options.Add(structureOption);
rootCommand.Options.Add(beforeOption);
rootCommand.Options.Add(changesOption);
rootCommand.Options.Add(afterOption);
rootCommand.Options.Add(outputOption);

rootCommand.SetAction(
  parseResult =>
  {
    var db = parseResult.GetValue(dbOption)!;
    var queryOptionValue = parseResult.GetValue(queryOption) ?? "";
    var queryArgumentValue = parseResult.GetValue(queryArgument) ?? "";
    var trace = parseResult.GetValue(traceOption);
    var autoCreateMissingReferences = parseResult.GetValue(autoCreateMissingReferencesOption);
    var structure = parseResult.GetValue(structureOption);
    var before = parseResult.GetValue(beforeOption);
    var changes = parseResult.GetValue(changesOption);
    var after = parseResult.GetValue(afterOption);
    var outputPath = parseResult.GetValue(outputOption);

    var decoratedLinks = new NamedTypesDecorator<uint>(db, trace);

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
        return 1;
      }

      return TryWriteLinoOutput(decoratedLinks, outputPath) ? 0 : 1;
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

    return TryWriteLinoOutput(decoratedLinks, outputPath) ? 0 : 1;
  }
);

return rootCommand.Parse(args).Invoke();

static void PrintAllLinks(INamedTypesLinks<uint> links)
{
  LinoDatabaseOutput.WriteDatabase(links, Console.Out);
}

static void PrintChange(INamedTypesLinks<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
{
  Console.WriteLine(LinoDatabaseOutput.FormatChange(links, linkBefore, linkAfter));
}

static bool TryWriteLinoOutput(INamedTypesLinks<uint> links, string? outputPath)
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
    return false;
  }
}
