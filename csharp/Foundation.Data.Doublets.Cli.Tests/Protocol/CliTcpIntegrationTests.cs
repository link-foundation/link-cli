using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary><c>clink --serve</c> and <c>clink --connect</c> end to end (issue #105).</summary>
public sealed class CliTcpIntegrationTests : IDisposable
{
    private const string Banner = "clink server listening on ";
    private readonly string _directory = Directory.CreateTempSubdirectory("clink-tcp-").FullName;

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private sealed class Server : IDisposable
    {
        private readonly Process _process;

        private Server(Process process, string address)
        {
            _process = process;
            Address = address;
        }

        public string Address { get; }

        public static async Task<Server> StartAsync(string database, params string[] extra)
        {
            var process = Process.Start(Clink.StartInfo(new[] { "--db", database, "--serve", "127.0.0.1:0" }.Concat(extra)))!;
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
            var line = await process.StandardOutput.ReadLineAsync(timeout.Token);
            if (line is null || !line.StartsWith(Banner, StringComparison.Ordinal))
            {
                process.Kill(entireProcessTree: true);
                process.Dispose();
                throw new InvalidOperationException($"unexpected banner '{line}'");
            }
            return new Server(process, line[Banner.Length..]);
        }

        public async Task<string> StdoutAsync(params string[] arguments)
        {
            var (exitCode, stdout, stderr) = await Clink.RunAsync(new[] { "--connect", Address }.Concat(arguments).ToArray());
            Assert.True(exitCode == 0, $"{string.Join(' ', arguments)}: {stderr}");
            return stdout;
        }

        public void Dispose()
        {
            _process.Kill(entireProcessTree: true);
            _process.WaitForExit();
            _process.Dispose();
        }
    }

    [Fact]
    public async Task ClientsQueryAServedDatabaseOverBothProtocols()
    {
        using var server = await Server.StartAsync(Path.Combine(_directory, "served.links"));

        Assert.Equal("() ((1: 1 1))\n", await server.StdoutAsync("() ((1 1))"));
        Assert.Equal("() ((2: 2 2))\n", await server.StdoutAsync("--protocol", "binary", "() ((2 2))"));
        Assert.Equal(
            "((1: 1 1)) ((1: 1 2))\n",
            await server.StdoutAsync("--external-references", "--sequences", "((1: 1 1)) ((1: 1 2))"));
        Assert.Equal("(1: 1 2)\n(2: 2 2)\n", await server.StdoutAsync());
        Assert.Equal(
            "((1: 1 2)) ((1: 1 2))\n",
            await server.StdoutAsync("--progressive-widths", "--query", "((1: 1 2)) ((1: 1 2))"));
        // Deleting 2 also deletes 1, which refers to it, exactly like the Rust port.
        Assert.Equal("((2: 2 2)) ()\n((1: 1 2)) ()\n", await server.StdoutAsync("--protocol", "binary", "((2: 2 2)) ()"));
        Assert.Equal("", await server.StdoutAsync("--protocol", "text"));

        var failure = await Clink.RunAsync("--connect", server.Address, "((99: 1 1)) ()");
        Assert.NotEqual(0, failure.ExitCode);
        Assert.Contains("server error", failure.Stderr, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ServedChangesArePersisted()
    {
        var database = Path.Combine(_directory, "served.links");
        using (var server = await Server.StartAsync(database, "--auto-create-missing-references"))
        {
            await server.StdoutAsync("() ((child: father mother))");
        }
        var (exitCode, stdout, _) = await Clink.RunAsync("--db", database, "--after");
        Assert.Equal(0, exitCode);
        Assert.Contains("(child: father mother)", stdout, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("--serve", "127.0.0.1:0", "--connect", "127.0.0.1:1")]
    [InlineData("--connect", "127.0.0.1:1", "--protocol", "text", "--sequences")]
    [InlineData("--connect", "127.0.0.1:1", "--protocol", "udp")]
    [InlineData("--serve", "127.0.0.1:0", "--protocol", "udp")]
    [InlineData("--serve", "127.0.0.1:0", "() ((1 1))")]
    public async Task InvalidCombinationsAreRejected(params string[] arguments)
    {
        var (exitCode, _, stderr) = await Clink.RunAsync(new[] { "--db", Path.Combine(_directory, "x.links") }.Concat(arguments).ToArray());
        Assert.NotEqual(0, exitCode);
        Assert.False(string.IsNullOrWhiteSpace(stderr));
    }

    [Fact]
    public async Task HelpListsTheTcpOptions()
    {
        var (_, stdout, _) = await Clink.RunAsync("--help");
        foreach (var option in new[] { "--serve", "--connect", "--protocol", "--external-references", "--sequences", "--progressive-widths" })
        {
            Assert.Contains(option, stdout, StringComparison.Ordinal);
        }
    }
}
