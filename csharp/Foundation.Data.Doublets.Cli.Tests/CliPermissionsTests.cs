using System.Diagnostics;
using Platform.Data;
using World = Foundation.Data.Doublets.Cli.Tests.PermissionsDecoratorTests.World;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary><c>clink --permissions</c>, <c>--subject</c> and the token table end to end (issue #149).</summary>
public sealed class CliPermissionsTests : IDisposable
{
    private const string Banner = "clink server listening on ";
    private readonly string _directory = Directory.CreateTempSubdirectory("clink-permissions-").FullName;

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private string Data => Path.Combine(_directory, "data.links");

    private string Rules => Path.Combine(_directory, "rules.links");

    /// <summary>Writes the world of the permission tests into the data, its rules there or in a file of their own.</summary>
    private (Dictionary<string, uint> T, Dictionary<string, uint> V, List<string> AliceSees, List<string> All) MakeWorld(bool separateRules)
    {
        using var data = new NamedTypesDecorator<uint>(Data);
        using var rules = new NamedTypesDecorator<uint>(Rules);
        var world = PermissionsDecoratorTests.MakeWorld(data, separateRules ? rules : data);
        return (world.T, world.V, Lines(world, world.As(world.V["alice"])), Lines(world, data));
    }

    private static List<string> Lines(World world, INamedTypesLinks<uint> links)
    {
        using var output = new StringWriter();
        LinoDatabaseOutput.WriteDatabase(links, output);
        return output.ToString().Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(line => line.TrimEnd('\r')).ToList();
    }

    private static List<string> Lines(string stdout) => stdout.Split('\n', StringSplitOptions.RemoveEmptyEntries).ToList();

    private string[] Permissions(bool separateRules, params string[] rest) =>
        new[] { "--db", Data, "--permissions", separateRules ? Rules : "self" }.Concat(rest).ToArray();

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ASubjectSeesWhatItsRulesAllowAndWritesNothingElse(bool separateRules)
    {
        var (t, v, aliceSees, all) = MakeWorld(separateRules);
        // Without --permissions nothing changes: every link is there.
        var plain = await Clink.RunAsync("--db", Data, "--after");
        Clink.AssertSucceeded(plain);
        Assert.Equal(all, Lines(plain.Stdout));

        var seen = await Clink.RunAsync(Permissions(separateRules, "--subject", "alice", "--after"));
        Clink.AssertSucceeded(seen);
        Assert.Equal(aliceSees, Lines(seen.Stdout));
        Assert.Contains("(note: Note note)", aliceSees);
        Assert.Contains("(shared: Shared shared)", aliceSees);
        Assert.DoesNotContain(aliceSees, line => line.Contains("secret", StringComparison.Ordinal));

        // A note of alice's may become another note, not what she only reads.
        var refused = await Clink.RunAsync(Permissions(separateRules, "--subject", "alice", "--changes", "((note: Note note)) ((note: Public note))"));
        Assert.Equal(1, refused.ExitCode);
        Assert.Contains($"Forbidden: update {v["note"]}", refused.Stderr, StringComparison.Ordinal);
        var after = await Clink.RunAsync("--db", Data, "--after");
        Assert.Equal(all, Lines(after.Stdout));

        // A secret is as missing to alice as a deleted link.
        var hidden = await Clink.RunAsync(Permissions(separateRules, "--subject", v["alice"].ToString(), "--changes", "((secret: * *)) ()"));
        Assert.Equal(1, hidden.ExitCode);
        Assert.Contains("Invalid reference to non-existent link 'secret'", hidden.Stderr, StringComparison.Ordinal);
        Assert.Equal(all, Lines((await Clink.RunAsync("--db", Data, "--after")).Stdout));

        // The boss writes anything.
        var boss = await Clink.RunAsync(Permissions(separateRules, "--subject", "boss", "--changes", "((note: Note note)) ((note: Secret note))"));
        Clink.AssertSucceeded(boss);
        Assert.Contains("((note: Note note)) ((note: Secret note))", boss.Stdout, StringComparison.Ordinal);
        Assert.NotEqual(0u, t["Secret"]);
    }

    [Fact]
    public async Task AGuestReadsNothingAndCreatesNothing()
    {
        MakeWorld(separateRules: false);
        var seen = await Clink.RunAsync(Permissions(false, "--after"));
        Clink.AssertSucceeded(seen);
        Assert.Equal("", seen.Stdout);
        // A guest creates nothing, so whether 1 exists makes no difference to the answer.
        foreach (var request in new[] { "() ((1 2))", "() ((99999 2))" })
        {
            var refused = await Clink.RunAsync(Permissions(false, request));
            Assert.Equal(1, refused.ExitCode);
            Assert.Contains("Forbidden: create", refused.Stderr, StringComparison.Ordinal);
        }
        var autoCreating = await Clink.RunAsync(Permissions(false, "--auto-create-missing-references", "() ((1 2))"));
        Assert.Equal(1, autoCreating.ExitCode);
        Assert.Contains("Forbidden: create", autoCreating.Stderr, StringComparison.Ordinal);
        var unknown = await Clink.RunAsync(Permissions(false, "--subject", "nobody", "--after"));
        Assert.Equal(1, unknown.ExitCode);
        Assert.Contains("Unknown subject 'nobody'", unknown.Stderr, StringComparison.Ordinal);
    }

    [Fact]
    public async Task TokenCommandsNeedATokenTableAndASubject()
    {
        MakeWorld(separateRules: false);
        var noTable = await Clink.RunAsync("--db", Data, "--subject", "alice", "--grant-token", "x");
        Assert.Equal(1, noTable.ExitCode);
        Assert.Contains("--grant-token and --revoke-token need --tokens.", noTable.Stderr, StringComparison.Ordinal);
        var noSubject = await Clink.RunAsync("--db", Data, "--tokens", "self", "--grant-token", "x");
        Assert.Equal(1, noSubject.ExitCode);
        Assert.Contains("--grant-token needs a --subject.", noSubject.Stderr, StringComparison.Ordinal);
        var unknown = await Clink.RunAsync("--db", Data, "--tokens", "self", "--revoke-token", "x");
        Clink.AssertSucceeded(unknown);
        Assert.Equal("Unknown token.\n", unknown.Stdout);
        var triggers = await Clink.RunAsync(Permissions(false, "--always", "() ((1 1))"));
        Assert.Equal(1, triggers.ExitCode);
        Assert.Contains("--always, --once and --never cannot be used with --permissions.", triggers.Stderr, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ServedConnectionsAuthenticateWithTokensStoredAsLinks(bool separateTokens)
    {
        var (_, v, aliceSees, all) = MakeWorld(separateRules: false);
        var tokens = separateTokens ? Path.Combine(_directory, "tokens.links") : "self";
        var granted = await Clink.RunAsync("--db", Data, "--tokens", tokens, "--subject", "alice", "--grant-token", "alice-secret");
        Clink.AssertSucceeded(granted);
        Assert.StartsWith($"Token granted to subject {v["alice"]}", granted.Stdout, StringComparison.Ordinal);
        if (!separateTokens)
        {
            // The token links are links of the data, which alice cannot see.
            all = Lines((await Clink.RunAsync("--db", Data, "--after")).Stdout);
        }

        using var server = await StartServerAsync("--permissions", "self", "--tokens", tokens);
        var guest = await Clink.RunAsync("--connect", server.Address, "(count: ())");
        Clink.AssertSucceeded(guest);
        Assert.Equal("(count: 0)\n", guest.Stdout);
        var alice = await Clink.RunAsync("--connect", server.Address, "--token", "alice-secret", "(count: ())");
        Clink.AssertSucceeded(alice);
        Assert.Equal($"(count: {aliceSees.Count})\n", alice.Stdout);
        var refused = await Clink.RunAsync("--connect", server.Address, "--token", "alice-secret", $"(update: ({v["note"]} {v["secret"]} {v["note"]}))");
        Assert.NotEqual(0, refused.ExitCode);
        var wrong = await Clink.RunAsync("--connect", server.Address, "--token", "wrong", "(count: ())");
        Assert.NotEqual(0, wrong.ExitCode);
        Assert.Contains("Forbidden: authentication failed", wrong.Stderr, StringComparison.Ordinal);
        server.Dispose();

        Assert.Equal(all, Lines((await Clink.RunAsync("--db", Data, "--after")).Stdout));
        var revoked = await Clink.RunAsync("--db", Data, "--tokens", tokens, "--revoke-token", "alice-secret");
        Clink.AssertSucceeded(revoked);
        Assert.Equal("Token revoked.\n", revoked.Stdout);
    }

    /// <summary>
    /// Issue #161: a subject without the right gets the same answer for an unreadable address
    /// and a missing one, and the store does not change.
    /// </summary>
    [Theory]
    [InlineData("() ((Note <address>))")]
    [InlineData("() ((<address>: Note Note))")]
    [InlineData("((<address>: * *)) ()")]
    [InlineData("((<address>: * *)) ((<address>: Note Note))")]
    public async Task AnUnreadableAddressAnswersAsAMissingOne(string query)
    {
        const uint missing = 9999;
        var (_, v, _, _) = MakeWorld(separateRules: false);
        Clink.AssertSucceeded(await Clink.RunAsync(Permissions(false, "--subject", "alice", "--after")));
        var before = Lines((await Clink.RunAsync("--db", Data, "--after")).Stdout);

        async Task<(int, string)> AnswerAt(uint address)
        {
            var answer = await Clink.RunAsync(Permissions(false, "--subject", "alice", "--changes", query.Replace("<address>", address.ToString())));
            return (answer.ExitCode, (answer.Stdout + answer.Stderr).Replace(address.ToString(), "<address>"));
        }

        Assert.Equal(await AnswerAt(missing), await AnswerAt(v["secret"]));
        Assert.Equal(before, Lines((await Clink.RunAsync("--db", Data, "--after")).Stdout));
    }

    /// <summary>The name of a hidden link answers as an unknown name.</summary>
    [Fact]
    public async Task TheNameOfAnUnreadableLinkAnswersAsAnUnknownName()
    {
        MakeWorld(separateRules: false);
        Clink.AssertSucceeded(await Clink.RunAsync(Permissions(false, "--subject", "alice", "--after")));
        var before = Lines((await Clink.RunAsync("--db", Data, "--after")).Stdout);
        var hidden = await Clink.RunAsync(Permissions(false, "--subject", "alice", "() ((Note secret))"));
        var unknown = await Clink.RunAsync(Permissions(false, "--subject", "alice", "() ((Note nothing))"));
        Assert.Equal(unknown.ExitCode, hidden.ExitCode);
        Assert.Equal(unknown.Stderr.Replace("nothing", "<name>"), hidden.Stderr.Replace("secret", "<name>"));
        Assert.Equal(before, Lines((await Clink.RunAsync("--db", Data, "--after")).Stdout));
    }

    private async Task<RunningClink> StartServerAsync(params string[] extra)
    {
        var process = Process.Start(Clink.StartInfo(new[] { "--db", Data, "--serve", "127.0.0.1:0" }.Concat(extra)))!;
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
        var line = await process.StandardOutput.ReadLineAsync(timeout.Token);
        if (line is null || !line.StartsWith(Banner, StringComparison.Ordinal))
        {
            var stderr = await process.StandardError.ReadToEndAsync(timeout.Token);
            process.Kill(entireProcessTree: true);
            process.Dispose();
            throw new InvalidOperationException($"unexpected banner '{line}': {stderr}");
        }
        return new RunningClink(process, line[Banner.Length..]);
    }

    private sealed class RunningClink(Process process, string address) : IDisposable
    {
        private bool _disposed;

        public string Address { get; } = address;

        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
            process.Kill(entireProcessTree: true);
            process.WaitForExit();
            process.Dispose();
        }
    }
}
