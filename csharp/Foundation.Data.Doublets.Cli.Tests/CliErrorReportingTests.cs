namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>A query clink cannot run is reported in one line on stderr, the way the Rust clink reports it.</summary>
public sealed class CliErrorReportingTests : IDisposable
{
    private readonly string _directory = Directory.CreateTempSubdirectory("clink-errors-").FullName;

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private Task<Clink.Result> RunQueryAsync(params string[] arguments) =>
        Clink.RunAsync(new[] { "--db", Path.Combine(_directory, "db.links") }.Concat(arguments).ToArray());

    [Fact]
    public async Task AMissingReferenceIsAQueryErrorWithoutAStackTrace()
    {
        var result = await RunQueryAsync("() ((2 2))");

        Assert.Equal(1, result.ExitCode);
        Assert.Equal(
            "Error: Query error: Invalid reference to non-existent link '2' in substitution pattern. " +
            "Link '2' does not exist and will not be created by this operation. " +
            "Use --auto-create-missing-references to create missing references as point links.\n",
            result.Stderr);
        Assert.Equal("", result.Stdout);
    }

    [Fact]
    public async Task AMalformedQueryIsAParseErrorPointingAtTheProblem()
    {
        var result = await RunQueryAsync("(((");

        Assert.Equal(1, result.ExitCode);
        Assert.StartsWith("Error: Parse error: Syntax error at line 1, column 4", result.Stderr, StringComparison.Ordinal);
        Assert.Contains("1 | (((\n  |    ^", result.Stderr, StringComparison.Ordinal);
        Assert.DoesNotContain("   at ", result.Stderr, StringComparison.Ordinal);
    }

    [Fact]
    public async Task AFailedQueryLeavesTheStoreUnchanged()
    {
        Clink.AssertSucceeded(await RunQueryAsync("() ((1 1))"));

        Assert.Equal(1, (await RunQueryAsync("() ((1 3))")).ExitCode);

        var after = await RunQueryAsync("--after");
        Clink.AssertSucceeded(after);
        Assert.Equal("(1: 1 1)\n", after.Stdout);
    }

    [Fact]
    public async Task TraceKeepsTheStackTraceForDiagnosis()
    {
        var result = await RunQueryAsync("--trace", "() ((2 2))");

        Assert.Equal(1, result.ExitCode);
        Assert.Contains("Error: Query error: Invalid reference to non-existent link '2'", result.Stderr, StringComparison.Ordinal);
        Assert.Contains("   at ", result.Stderr, StringComparison.Ordinal);
    }
}
