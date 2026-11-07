using System.Reflection;

namespace Foundation.Data.Doublets.Cli.Tests;

// Entry-point execution joins the existing serialized console collection. The
// separate subprocess integration tests continue to verify executable startup.
[Collection("Diagnostic console")]
public sealed class CliCommandContractTests : IDisposable
{
    private readonly string _directory = Directory.CreateTempSubdirectory("cli-command-contract-").FullName;
    private string Database => Path.Combine(_directory, "data.links");

    private Clink.Result Run(params string[] arguments)
    {
        var stdout = Console.Out;
        var stderr = Console.Error;
        using var output = new StringWriter();
        using var errors = new StringWriter();
        try
        {
            Console.SetOut(output);
            Console.SetError(errors);
            var entry = Assembly.Load("clink").EntryPoint!;
            var args = new[] { "--db", Database }.Concat(arguments).ToArray();
            var exit = Assert.IsType<int>(entry.Invoke(null, new object[] { args }));
            return new Clink.Result(exit, output.ToString().Replace("\r\n", "\n"),
                errors.ToString().Replace("\r\n", "\n"));
        }
        finally
        {
            Console.SetOut(stdout);
            Console.SetError(stderr);
            // Every returned command, including an error, releases its mapped
            // stores. A following invocation must not depend on finalization.
            foreach (var file in Directory.GetFiles(_directory, "*.links"))
            {
                using var exclusive = new FileStream(file, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
                Assert.True(exclusive.CanWrite);
            }
        }
    }

    private string Succeed(params string[] arguments)
    {
        var result = Run(arguments);
        Clink.AssertSucceeded(result);
        Assert.Equal("", result.Stderr);
        return result.Stdout;
    }

    private void AssertBaseline()
    {
        Assert.Equal("(1: 1 1)\n", Succeed("--after"));
        Succeed("() ((2 2))");
        Assert.Equal("(1: 1 1)\n(2: 2 2)\n", Succeed("--after"));
    }

    [Theory]
    [InlineData("commit", "Invalid --commit-mode value")]
    [InlineData("retention", "Invalid --retention value")]
    [InlineData("triggers", "Only one of --always, --once, or --never")]
    [InlineData("missing-trigger-query", "require a query")]
    [InlineData("serve-query", "--serve does not take a query")]
    [InlineData("serve-protocol", "Invalid --protocol value")]
    [InlineData("serve-connect", "--serve and --connect cannot be used together")]
    [InlineData("client-protocol", "Invalid --protocol value")]
    [InlineData("client-options", "Binary protocol options require")]
    [InlineData("client-address", "Error:")]
    [InlineData("arity", "invalid arity")]
    [InlineData("checkout-missing", "Unknown checkout point")]
    [InlineData("checkout-negative", "Error during --checkout")]
    [InlineData("branch-negative", "Error creating branch")]
    [InlineData("tag-missing", "Unknown tag point")]
    [InlineData("tag-trailing", "Unknown tag point ''")]
    [InlineData("tag-empty", "Error creating tag")]
    [InlineData("tag-negative", "Error creating tag")]
    [InlineData("query-parse", "Parse error")]
    [InlineData("query-reference", "Query error")]
    public void RejectedCommandPreservesRowsReleasesFilesAndAllowsFollowingEdits(string scenario, string diagnostic)
    {
        Succeed("() ((1 1))");
        string[] args = scenario switch
        {
            "commit" => ["--commit-mode", "invalid"],
            "retention" => ["--retention", "invalid"],
            "triggers" => ["--always", "--once"],
            "missing-trigger-query" => ["--never"],
            "serve-query" => ["--serve", "127.0.0.1:0", "() ((2 2))"],
            "serve-protocol" => ["--serve", "127.0.0.1:0", "--protocol", "udp"],
            "serve-connect" => ["--serve", "127.0.0.1:0", "--connect", "127.0.0.1:1"],
            "client-protocol" => ["--connect", "invalid-address", "--protocol", "udp"],
            "client-options" => ["--connect", "invalid-address", "--protocol", "text", "--packed-widths"],
            "client-address" => ["--connect", "invalid-address"],
            "arity" => ["--arity", "2..x"],
            "checkout-missing" => ["--checkout", "missing"],
            "checkout-negative" => ["--checkout", "-1"],
            "branch-negative" => ["--branch", "new", "--branch-from", "-1"],
            "tag-missing" => ["--tag", "new=missing"],
            "tag-trailing" => ["--tag", "new="],
            "tag-empty" => ["--tag", "=0"],
            "tag-negative" => ["--tag", "new=-1"],
            "query-parse" => ["--trace", "((("],
            _ => ["--trace", "() ((99 99))"]
        };
        var result = Run(args);
        Assert.Equal(1, result.ExitCode);
        Assert.Contains(diagnostic, result.Stderr, StringComparison.OrdinalIgnoreCase);
        AssertBaseline();
    }

    [Theory]
    [InlineData("--import", "Error reading LiNo input")]
    [InlineData("--import-binary", "Error reading binary store archive")]
    [InlineData("--export", "Error writing LiNo output")]
    [InlineData("--export-binary", "Error writing binary store archive")]
    public void FileErrorsPreserveDataAndReleaseTheCommand(string option, string diagnostic)
    {
        Succeed("() ((1 1))");
        var path = Path.Combine(_directory, "absent", "file");
        var result = Run(option, path);
        Assert.Equal(1, result.ExitCode);
        Assert.Contains(diagnostic, result.Stderr, StringComparison.Ordinal);
        AssertBaseline();
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void VersionsRetainNamesAndRowsAcrossRepeatedCommandOwnership(bool asynchronous)
    {
        Succeed("--commit-mode", asynchronous ? "async" : "sync", "--vc", "() ((1 1))");
        Assert.Contains("Tagged 'first'", Succeed("--trace", "--tag", "first"), StringComparison.Ordinal);
        var tag = Succeed("--list-tags");
        var head = long.Parse(tag.Trim().Split('\t')[1], System.Globalization.CultureInfo.InvariantCulture);
        Assert.Contains("Tagged 'alias'", Succeed("--trace", "--tag", "alias=first"), StringComparison.Ordinal);
        var branch = Succeed("--trace", "--branch", "topic:parent=雪", "--branch-from", head.ToString(), "() ((2 2))");
        Assert.Contains("Created branch 'topic:parent=雪'", branch, StringComparison.Ordinal);
        Assert.Contains("Switched to branch 'topic:parent=雪'", branch, StringComparison.Ordinal);
        var branches = Succeed("--list-branches");
        Assert.Contains("* topic:parent=雪\tparent=main", branches, StringComparison.Ordinal);
        var checkout = Succeed("--trace", "--checkout", "alias", "--after");
        Assert.Contains("Checked out seq", checkout, StringComparison.Ordinal);
        Assert.Contains("(1: 1 1)\n", checkout, StringComparison.Ordinal);
        Assert.DoesNotContain("(2: 2 2)", checkout, StringComparison.Ordinal);
        Assert.Contains("Switched to branch 'main'", Succeed("--trace", "--branch", "main"), StringComparison.Ordinal);
        Succeed("--vc", "() ((2 2))");
        var history = Succeed("--log");
        Assert.Contains("\tCreate\t", history, StringComparison.Ordinal);
        Assert.Contains(" -> (2,2,2)", history, StringComparison.Ordinal);
        Assert.Contains($"alias\t{head}\n", Succeed("--list-tags"), StringComparison.Ordinal);
        Assert.Equal("(1: 1 1)\n(2: 2 2)\n", Succeed("--after"));
    }

    [Theory]
    [InlineData("release=雪=part")]
    [InlineData("release=0")]
    public void TagNamesContainingEqualsAndAliasesToThemSurviveCommands(string name)
    {
        Succeed("--vc", "() ((1 1))");
        Succeed("--tag", "first");
        var head = Succeed("--list-tags").Trim().Split('\t')[1];
        Succeed("--tag", $"{name}={head}");
        Assert.Contains($"{name}\t{head}\n", Succeed("--list-tags"), StringComparison.Ordinal);
        // The old first-delimiter syntax already accepts an existing full tag
        // as its point. Preserve that interpretation when adding named prefixes.
        Succeed("--tag", $"alias={name}");
        var listing = Succeed("--list-tags");
        Assert.Equal($"alias\t{head}\nfirst\t{head}\n{name}\t{head}\n", listing);
        Succeed("--vc", "() ((2 2))");
        Assert.Equal("(1: 1 1)\n", Succeed("--checkout", name, "--after"));
        Assert.Equal("(1: 1 1)\n", Succeed("--checkout", "alias", "--after"));
        var rejected = Run("--tag", "new=still=missing");
        Assert.Equal(1, rejected.ExitCode);
        Assert.Contains("Unknown tag point 'still=missing'.", rejected.Stderr, StringComparison.Ordinal);
        Assert.Equal(listing, Succeed("--list-tags"));
        Succeed("--vc", "() ((2 2))");
        Assert.Equal("(1: 1 1)\n(2: 2 2)\n", Succeed("--after"));
    }

    [Theory]
    [InlineData("--always")]
    [InlineData("--once")]
    public void StoredTriggersCanBeRemovedBeforeTheNextDataCommand(string kind)
    {
        Succeed("() ((1 1))");
        const string query = "() ((2 2))";
        Assert.Contains("persistent transformation trigger stored:", Succeed(kind, query), StringComparison.Ordinal);
        Assert.Contains("Persistent transformation triggers removed: 1", Succeed("--never", query), StringComparison.Ordinal);
        Assert.Equal("(1: 1 1)\n", Succeed("--triggers", "--after"));
        AssertBaseline();
    }

    [Fact]
    public void QueryPipelineImportsBeforeQueryAndExportsTheSameFinalRows()
    {
        var input = Path.Combine(_directory, "input.lino");
        var output = Path.Combine(_directory, "output.lino");
        var archive = Path.Combine(_directory, "output.bin");
        File.WriteAllText(input, "(1: 1 1)\n");
        var trace = Succeed("--trace", "--before", "--changes", "--after", "--import", input,
            "--export", output, "--export-binary", archive, "() ((2 2))");
        Assert.Contains("[DEBUG] Raw changes before simplification:", trace, StringComparison.Ordinal);
        Assert.Contains("[DEBUG] Simplified changes count:", trace, StringComparison.Ordinal);
        Assert.Contains("(1: 1 1)\n(2: 2 2)\n", trace, StringComparison.Ordinal);
        Assert.Equal("(1: 1 1)\n(2: 2 2)\n", File.ReadAllText(output).Replace("\r\n", "\n"));
        Assert.NotEmpty(File.ReadAllBytes(archive));
        Assert.Equal("(1: 1 1)\n", Succeed("--structure", "1", "--export", output));
        Assert.Equal("(1: 1 1)\n(2: 2 2)\n", Succeed("--import-binary", archive, "--after"));
    }

    public void Dispose() => Directory.Delete(_directory, recursive: true);
}
