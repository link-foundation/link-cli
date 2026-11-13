using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>
/// <c>--serve</c> on several endpoints, <c>--forward</c> and <c>--connect</c> over any IPC endpoint (issue #140),
/// as the Rust <c>clink</c> does them.
/// </summary>
public sealed class CliIpcEndpointsTests : IDisposable
{
    private const string Banner = "clink server listening on ";
    private readonly string _directory = Directory.CreateTempSubdirectory("cli-ipc-").FullName;

    private string Data => Path.Combine(_directory, "data.links");

    private string Dir(string name) => Path.Combine(_directory, name);

    [Theory]
    [InlineData("--forward 127.0.0.1:1", "--forward relays the connections of --serve; give --serve as well.")]
    [InlineData("--serve 127.0.0.1:0 --forward 127.0.0.1:1 () ((1 1))", "--forward does not take a query; send queries with --connect.")]
    [InlineData("--serve 127.0.0.1:0 --forward 127.0.0.1:1 --graphql", "--forward and --graphql cannot be used together.")]
    [InlineData("--serve 127.0.0.1:0 --forward exec:", "Error: ")]
    [InlineData("--serve nonsense: --forward 127.0.0.1:1", "Error: ")]
    [InlineData("--serve 127.0.0.1:0 --serve 127.0.0.1:0 --graphql", "--graphql serves HTTP on one TCP address; give --serve once.")]
    [InlineData("--serve file:DIR --graphql", "--graphql serves HTTP over TCP only, not over `file:`.")]
    [InlineData("--serve 127.0.0.1:0 --connect 127.0.0.1:1", "--serve and --connect cannot be used together.")]
    public async Task EndpointMisuseIsReportedAndRefused(string arguments, string expected)
    {
        var words = arguments.Replace("DIR", Dir("graphql"), StringComparison.Ordinal).Split(' ');
        var query = Array.IndexOf(words, "()");
        if (query >= 0)
        {
            words = [.. words[..query], string.Join(' ', words[query..])];
        }
        var result = await Clink.RunAsync(["--db", Data, .. words]);
        Assert.Equal(1, result.ExitCode);
        Assert.Contains(expected, result.Stderr, StringComparison.Ordinal);
    }

    [Fact(Timeout = 120000)]
    public async Task OneServerListensOnEveryServeEndpointAndARelayStacksAnotherOnTop()
    {
        var files = $"file:{Dir("files")}";
        var relayed = $"file:{Dir("relay")}";
        using var server = await StartAsync(2, "--db", Data, "--serve", "127.0.0.1:0", "--serve", files);
        Assert.Equal(files, server.Endpoints[1]);
        var tcp = server.Endpoints[0];

        Clink.AssertSucceeded(await Clink.RunAsync("--connect", tcp, "() ((1 1))"));
        Clink.AssertSucceeded(await Clink.RunAsync("--connect", files, "--protocol", "binary", "() ((2 2))"));

        using var relay = await StartAsync(1, "--serve", relayed, "--forward", tcp, "--trace");
        Assert.Equal([relayed], relay.Endpoints);
        var read = await Clink.RunAsync("--connect", relayed, "--trace", "");
        Clink.AssertSucceeded(read);
        Assert.Equal("(1: 1 1)\n(2: 2 2)\n", read.Stdout);
        Assert.Contains("[ipc]", read.Stderr, StringComparison.Ordinal);
        Assert.False(File.Exists(Path.Combine(_directory, "db.links")), "a relay opens no database");
    }

    private async Task<Running> StartAsync(int endpoints, params string[] arguments)
    {
        var info = Clink.StartInfo(arguments);
        info.RedirectStandardInput = true;
        info.WorkingDirectory = _directory;
        var process = Process.Start(info)!;
        var running = new Running(process);
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
        while (running.Endpoints.Count < endpoints)
        {
            var line = await process.StandardOutput.ReadLineAsync(timeout.Token);
            if (line is null || !line.StartsWith(Banner, StringComparison.Ordinal))
            {
                var stderr = await process.StandardError.ReadToEndAsync(timeout.Token);
                running.Dispose();
                throw new InvalidOperationException($"unexpected banner '{line}': {stderr}");
            }
            running.Endpoints.Add(line[Banner.Length..]);
        }
        return running;
    }

    private sealed class Running(Process process) : IDisposable
    {
        public List<string> Endpoints { get; } = [];

        public void Dispose()
        {
            process.Kill(entireProcessTree: true);
            process.WaitForExit();
            process.Dispose();
        }
    }

    public void Dispose() => Directory.Delete(_directory, recursive: true);
}
