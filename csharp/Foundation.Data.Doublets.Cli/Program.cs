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

var alwaysOption = new Option<bool>("--always")
{
    Description = "Store the query as an always-on persistent transformation trigger",
    DefaultValueFactory = _ => false
};

var onceOption = new Option<bool>("--once")
{
    Description = "Store the query as a persistent transformation trigger that deletes itself after it fires",
    DefaultValueFactory = _ => false
};

var neverOption = new Option<bool>("--never")
{
    Description = "Remove stored persistent transformation triggers matching the query",
    DefaultValueFactory = _ => false
};

var triggersOption = new Option<bool>("--triggers")
{
    Description = "Enable persistent transformation triggers for this command",
    DefaultValueFactory = _ => false
};

var triggersFileOption = new Option<string?>("--triggers-file")
{
    Description = "Path to the persistent transformation trigger links database"
};

var embedTriggersOption = new Option<bool>("--embed-triggers")
{
    Description = "Store persistent transformation triggers directly in the main links database",
    DefaultValueFactory = _ => false
};

var inputOption = new Option<string?>("--in", "--lino-input", "--import")
{
    Description = "Path to read and import a LiNo file into the database"
};

var transactionsOption = new Option<bool>("--transactions")
{
    Description = "Enable the transactions layer (default log path: <db>.transitions.links)",
    DefaultValueFactory = _ => false
};

var transactionsFileOption = new Option<string?>("--transactions-file")
{
    Description = "Path to the transitions log store (default: <db>.transitions.links). Implies --transactions."
};

var commitModeOption = new Option<string?>("--commit-mode")
{
    Description = "Choose 'sync' or 'async' commits (default: sync). Implies --transactions."
};

var retentionOption = new Option<string?>("--retention")
{
    Description = "Log retention policy: 'infinite', 'sized:<n>', or 'chunked:<n>:<dir>'. Implies --transactions."
};

var vcOption = new Option<bool>("--vc")
{
    Description = "Enable the version-control decorator (implies --transactions)",
    DefaultValueFactory = _ => false
};

var vcFileOption = new Option<string?>("--vc-file")
{
    Description = "Path to the version-control branches store (default: <db>.versioncontrol.links)"
};

var branchOption = new Option<string?>("--branch")
{
    Description = "Switch to a branch (creating it if --branch-from is also passed). Implies --vc."
};

var branchFromOption = new Option<long?>("--branch-from")
{
    Description = "When creating a branch with --branch, fork from this sequence point."
};

var checkoutOption = new Option<string?>("--checkout")
{
    Description = "Time-travel to a specific transition sequence or named tag. Implies --vc."
};

var tagOption = new Option<string?>("--tag")
{
    Description = "Create a tag in the form 'name' (at current head) or 'name=<seq>'. Implies --vc."
};

var listBranchesOption = new Option<bool>("--list-branches")
{
    Description = "List version-control branches and exit.",
    DefaultValueFactory = _ => false
};

var listTagsOption = new Option<bool>("--list-tags")
{
    Description = "List version-control tags and exit.",
    DefaultValueFactory = _ => false
};

var logOption = new Option<bool>("--log")
{
    Description = "Print the transitions log and exit. Implies --transactions.",
    DefaultValueFactory = _ => false
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
rootCommand.Options.Add(alwaysOption);
rootCommand.Options.Add(onceOption);
rootCommand.Options.Add(neverOption);
rootCommand.Options.Add(triggersOption);
rootCommand.Options.Add(triggersFileOption);
rootCommand.Options.Add(embedTriggersOption);
rootCommand.Options.Add(inputOption);
rootCommand.Options.Add(outputOption);
rootCommand.Options.Add(transactionsOption);
rootCommand.Options.Add(transactionsFileOption);
rootCommand.Options.Add(commitModeOption);
rootCommand.Options.Add(retentionOption);
rootCommand.Options.Add(vcOption);
rootCommand.Options.Add(vcFileOption);
rootCommand.Options.Add(branchOption);
rootCommand.Options.Add(branchFromOption);
rootCommand.Options.Add(checkoutOption);
rootCommand.Options.Add(tagOption);
rootCommand.Options.Add(listBranchesOption);
rootCommand.Options.Add(listTagsOption);
rootCommand.Options.Add(logOption);

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
      var always = parseResult.GetValue(alwaysOption);
      var once = parseResult.GetValue(onceOption);
      var never = parseResult.GetValue(neverOption);
      var triggers = parseResult.GetValue(triggersOption);
      var triggersFile = parseResult.GetValue(triggersFileOption);
      var embedTriggers = parseResult.GetValue(embedTriggersOption);
      var inputPath = parseResult.GetValue(inputOption);
      var outputPath = parseResult.GetValue(outputOption);
      var transactionsFlag = parseResult.GetValue(transactionsOption);
      var transactionsPathRaw = parseResult.GetValue(transactionsFileOption);
      var commitModeRaw = parseResult.GetValue(commitModeOption);
      var retentionRaw = parseResult.GetValue(retentionOption);
      var vc = parseResult.GetValue(vcOption);
      var vcFile = parseResult.GetValue(vcFileOption);
      var branchName = parseResult.GetValue(branchOption);
      var branchFrom = parseResult.GetValue(branchFromOption);
      var checkoutPoint = parseResult.GetValue(checkoutOption);
      var tagSpec = parseResult.GetValue(tagOption);
      var listBranches = parseResult.GetValue(listBranchesOption);
      var listTags = parseResult.GetValue(listTagsOption);
      var showLog = parseResult.GetValue(logOption);

      var triggerCommandCount = new[] { always, once, never }.Count(value => value);
      if (triggerCommandCount > 1)
      {
          Console.Error.WriteLine("Only one of --always, --once, or --never can be used at a time.");
          return 1;
      }

      var vcRequested = vc
        || !string.IsNullOrWhiteSpace(vcFile)
        || !string.IsNullOrWhiteSpace(branchName)
        || branchFrom.HasValue
        || !string.IsNullOrWhiteSpace(checkoutPoint)
        || !string.IsNullOrWhiteSpace(tagSpec)
        || listBranches
        || listTags;

      var transactionsRequested = transactionsFlag
        || !string.IsNullOrWhiteSpace(transactionsPathRaw)
        || !string.IsNullOrWhiteSpace(commitModeRaw)
        || !string.IsNullOrWhiteSpace(retentionRaw)
        || showLog
        || vcRequested;

      CommitMode commitMode = CommitMode.Sync;
      if (!string.IsNullOrWhiteSpace(commitModeRaw))
      {
          if (commitModeRaw.Equals("sync", StringComparison.OrdinalIgnoreCase))
          {
              commitMode = CommitMode.Sync;
          }
          else if (commitModeRaw.Equals("async", StringComparison.OrdinalIgnoreCase))
          {
              commitMode = CommitMode.Async;
          }
          else
          {
              Console.Error.WriteLine($"Invalid --commit-mode value '{commitModeRaw}'. Use 'sync' or 'async'.");
              return 1;
          }
      }

      LogRetentionPolicy? retentionPolicy = null;
      if (!string.IsNullOrWhiteSpace(retentionRaw))
      {
          try
          {
              retentionPolicy = LogRetentionPolicy.Parse(retentionRaw);
          }
          catch (ArgumentException ex)
          {
              Console.Error.WriteLine($"Invalid --retention value: {ex.Message}");
              return 1;
          }
      }

      var baseLinks = new NamedTypesDecorator<uint>(db, trace);
      INamedTypesLinks<uint> decoratedLinks = baseLinks;
      NamedTypesDecorator<uint>? transitionsStore = null;
      NamedTypesDecorator<uint>? vcBranchesStore = null;
      TransactionsDecorator? transactionsLinks = null;
      VersionControlDecorator? vcLinks = null;

      if (transactionsRequested)
      {
          var effectiveTransactionsFile = !string.IsNullOrWhiteSpace(transactionsPathRaw)
            ? transactionsPathRaw
            : TransactionsDecorator.MakeTransitionsDatabaseFilename(db);
          transitionsStore = new NamedTypesDecorator<uint>(effectiveTransactionsFile, trace);
          transactionsLinks = new TransactionsDecorator(
            baseLinks,
            transitionsStore,
            retentionPolicy,
            commitMode,
            trace);
          decoratedLinks = transactionsLinks;
      }

      if (vcRequested)
      {
          if (transactionsLinks is null)
          {
              Console.Error.WriteLine("--vc requires the transactions layer (this should have been auto-enabled).");
              return 1;
          }
          var effectiveVcFile = !string.IsNullOrWhiteSpace(vcFile)
            ? vcFile
            : VersionControlDecorator.MakeVersionControlDatabaseFilename(db);
          vcBranchesStore = new NamedTypesDecorator<uint>(effectiveVcFile, trace);
          vcLinks = new VersionControlDecorator(transactionsLinks, vcBranchesStore, trace);
          decoratedLinks = vcLinks;
      }

      PersistentTransformationDecorator? persistentLinks = null;
      var defaultTriggersFile = PersistentTransformationDecorator.MakeTriggersDatabaseFilename(db);
      var effectiveTriggersFile = string.IsNullOrWhiteSpace(triggersFile) ? defaultTriggersFile : triggersFile;
      var persistentTransformationsEnabled = always
        || once
        || never
        || triggers
        || embedTriggers
        || !string.IsNullOrWhiteSpace(triggersFile)
        || File.Exists(effectiveTriggersFile);

      if (persistentTransformationsEnabled)
      {
          var triggerLinks = embedTriggers
            ? (INamedTypesLinks<uint>)baseLinks
            : new NamedTypesDecorator<uint>(effectiveTriggersFile, trace);
          persistentLinks = new PersistentTransformationDecorator(decoratedLinks, triggerLinks, trace)
          {
              AutoCreateMissingReferences = autoCreateMissingReferences
          };
          decoratedLinks = persistentLinks;
      }

      try
      {
          return RunCli();
      }
      finally
      {
          transactionsLinks?.Shutdown();
      }

      int RunCli()
      {
          if (vcLinks is not null)
          {
              if (!string.IsNullOrWhiteSpace(checkoutPoint))
              {
                  if (!TryResolveSequence(vcLinks, checkoutPoint, out var seq))
                  {
                      Console.Error.WriteLine($"Unknown checkout point '{checkoutPoint}'.");
                      return 1;
                  }
                  try
                  {
                      vcLinks.Checkout(seq);
                      if (trace) Console.WriteLine($"Checked out seq {seq} on branch '{vcLinks.CurrentBranch}'.");
                  }
                  catch (Exception ex)
                  {
                      Console.Error.WriteLine($"Error during --checkout: {ex.Message}");
                      return 1;
                  }
              }

              if (!string.IsNullOrWhiteSpace(branchName))
              {
                  var existing = vcLinks.ListBranches().Any(b => b.Name == branchName);
                  if (!existing)
                  {
                      try
                      {
                          vcLinks.Branch(branchName, branchFrom);
                          if (trace) Console.WriteLine($"Created branch '{branchName}'.");
                      }
                      catch (Exception ex)
                      {
                          Console.Error.WriteLine($"Error creating branch '{branchName}': {ex.Message}");
                          return 1;
                      }
                  }
                  try
                  {
                      vcLinks.SwitchBranch(branchName);
                      if (trace) Console.WriteLine($"Switched to branch '{branchName}'.");
                  }
                  catch (Exception ex)
                  {
                      Console.Error.WriteLine($"Error switching to branch '{branchName}': {ex.Message}");
                      return 1;
                  }
              }

              if (!string.IsNullOrWhiteSpace(tagSpec))
              {
                  var eq = tagSpec.IndexOf('=');
                  string tagName;
                  long? tagSeq = null;
                  if (eq < 0)
                  {
                      tagName = tagSpec;
                  }
                  else
                  {
                      tagName = tagSpec.Substring(0, eq);
                      var point = tagSpec.Substring(eq + 1);
                      if (!TryResolveSequence(vcLinks, point, out var resolved))
                      {
                          Console.Error.WriteLine($"Unknown tag point '{point}'.");
                          return 1;
                      }
                      tagSeq = resolved;
                  }
                  try
                  {
                      vcLinks.Tag(tagName, tagSeq);
                      if (trace) Console.WriteLine($"Tagged '{tagName}' at seq {tagSeq ?? vcLinks.CurrentSequence}.");
                  }
                  catch (Exception ex)
                  {
                      Console.Error.WriteLine($"Error creating tag '{tagName}': {ex.Message}");
                      return 1;
                  }
              }

              if (listBranches)
              {
                  foreach (var info in vcLinks.ListBranches())
                  {
                      var marker = info.Name == vcLinks.CurrentBranch ? "*" : " ";
                      var parent = info.Parent ?? "-";
                      Console.WriteLine($"{marker} {info.Name}\tparent={parent}\tfork={info.ForkSeq}\thead={info.Head}");
                  }
                  return 0;
              }

              if (listTags)
              {
                  foreach (var tag in vcLinks.ListTags().OrderBy(t => t.Key, StringComparer.Ordinal))
                  {
                      Console.WriteLine($"{tag.Key}\t{tag.Value}");
                  }
                  return 0;
              }
          }

          if (showLog)
          {
              if (transactionsLinks is null)
              {
                  Console.Error.WriteLine("--log requires the transactions layer.");
                  return 1;
              }
              foreach (var transition in transactionsLinks.Log)
              {
                  Console.WriteLine($"{transition.Sequence}\t{transition.Timestamp:O}\t{transition.Kind}\t{transition.TransactionId:N}\t({transition.Before.Index},{transition.Before.Source},{transition.Before.Target}) -> ({transition.After.Index},{transition.After.Source},{transition.After.Target})");
              }
              return 0;
          }

          return RunQueryPipeline();
      }

      bool TryResolveSequence(VersionControlDecorator vc, string point, out long sequence)
      {
          sequence = 0;
          if (string.IsNullOrWhiteSpace(point)) return false;
          if (long.TryParse(point, System.Globalization.NumberStyles.Integer, System.Globalization.CultureInfo.InvariantCulture, out var direct))
          {
              sequence = direct;
              return true;
          }
          if (vc.TryGetTag(point, out var tagSeq))
          {
              sequence = tagSeq;
              return true;
          }
          return false;
      }

      int RunQueryPipeline()
      {

          if (before)
          {
              PrintAllLinks(decoratedLinks);
          }

          if (!TryReadLinoInput(decoratedLinks, inputPath))
          {
              return 1;
          }

          if (structure.HasValue)
          {
              var linkId = structure.Value;
              try
              {
                  var structureFormatted = LinoDatabaseOutput.FormatStructure(decoratedLinks, linkId);
                  Console.WriteLine(structureFormatted);
              }
              catch (Exception ex)
              {
                  Console.Error.WriteLine($"Error formatting structure for link ID {linkId}: {ex.Message}");
                  return 1;
              }

              return TryWriteLinoOutput(decoratedLinks, outputPath) ? 0 : 1;
          }

          var effectiveQuery = !string.IsNullOrWhiteSpace(queryOptionValue) ? queryOptionValue : queryArgumentValue;

          if ((always || once || never) && string.IsNullOrWhiteSpace(effectiveQuery))
          {
              Console.Error.WriteLine("--always, --once, and --never require a query.");
              return 1;
          }

          if (persistentLinks is not null && (always || once))
          {
              var kind = always ? PersistentTransformationKind.Always : PersistentTransformationKind.Once;
              var trigger = persistentLinks.StoreTrigger(kind, effectiveQuery);
              Console.WriteLine($"{kind} persistent transformation trigger stored: {trigger}");
              return TryWriteLinoOutput(decoratedLinks, outputPath) ? 0 : 1;
          }

          if (persistentLinks is not null && never)
          {
              var removed = persistentLinks.RemoveTriggers(effectiveQuery);
              Console.WriteLine($"Persistent transformation triggers removed: {removed}");
              return TryWriteLinoOutput(decoratedLinks, outputPath) ? 0 : 1;
          }

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

static bool TryReadLinoInput(INamedTypesLinks<uint> links, string? inputPath)
{
    if (string.IsNullOrWhiteSpace(inputPath))
    {
        return true;
    }

    try
    {
        LinoDatabaseInput.ReadFromFile(links, inputPath);
        return true;
    }
    catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException || ex is NotSupportedException || ex is FormatException)
    {
        Console.Error.WriteLine($"Error reading LiNo input file '{inputPath}': {ex.Message}");
        return false;
    }
}
