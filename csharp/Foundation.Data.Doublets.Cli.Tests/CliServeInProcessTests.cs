using System.Reflection;
using System.Runtime.InteropServices;

namespace Foundation.Data.Doublets.Cli.Tests;

// An interrupt reaches every handler of this process, so whatever serves from
// inside the test host shares this collection and never runs beside these.
[Collection("Diagnostic console")]
public sealed class CliServeInProcessTests : IDisposable
{
    private const int SigInt = 2;
    private const int SigTerm = 15;
    private readonly string _directory = Directory.CreateTempSubdirectory("cli-serve-").FullName;

    [DllImport("libc", EntryPoint = "kill", SetLastError = true)]
    private static extern int Kill(int process, int signal);

    private string[] Arguments(params string[] rest) =>
        new[] { "--db", Path.Combine(_directory, "data.links") }.Concat(rest).ToArray();

    private static int Invoke(string[] arguments) =>
        Assert.IsType<int>(Assembly.Load("clink").EntryPoint!.Invoke(null, new object[] { arguments }));

    private static (int Exit, string Output, string Errors) Run(params string[] arguments)
    {
        var (stdout, stderr) = (Console.Out, Console.Error);
        using var output = new StringWriter();
        using var errors = new StringWriter();
        try
        {
            Console.SetOut(output);
            Console.SetError(errors);
            return (Invoke(arguments), output.ToString(), errors.ToString());
        }
        finally
        {
            Console.SetOut(stdout);
            Console.SetError(stderr);
        }
    }

    [Theory]
    [InlineData("--query", "() ((1 1))", "does not take a query")]
    [InlineData("--protocol", "udp", "Invalid --protocol value 'udp'")]
    public void ServingRefusesWhatItCannotDo(string option, string value, string expected)
    {
        var (exit, _, errors) = Run(Arguments("--serve", "127.0.0.1:0", option, value));
        Assert.Equal(1, exit);
        Assert.Contains(expected, errors, StringComparison.Ordinal);
    }

    [Fact]
    public void AValidRetentionIsAccepted()
    {
        var (exit, _, errors) = Run(Arguments("--transactions", "--retention", "sized:1", "--query", "() ((1 1))"));
        Assert.Equal("", errors);
        Assert.Equal(0, exit);
    }

    [Fact]
    public void TheStructureOfALinkThatIsNotThereIsReportedAsAFailure()
    {
        var (exit, _, errors) = Run(Arguments("--structure", "999"));
        Assert.Equal(1, exit);
        Assert.Contains("Error formatting structure for link ID 999", errors, StringComparison.Ordinal);
    }

    [Theory(Timeout = 60000)]
    [InlineData("", "", SigInt)]
    [InlineData("--protocol", "text", SigInt)]
    [InlineData("--protocol", "binary", SigInt)]
    [InlineData("--graphql", "", SigInt)]
    [InlineData("--protocol", "text", SigTerm)]
    [InlineData("--graphql", "", SigTerm)]
    public async Task AStopSignalStopsTheServerAndLeavesTheProcessAlone(string option, string value, int signal)
    {
        // Windows delivers console events only to a console, not to one process.
        if (OperatingSystem.IsWindows()) return;

        var arguments = Arguments("--serve", "127.0.0.1:0");
        if (option.Length > 0) arguments = arguments.Append(option).ToArray();
        if (value.Length > 0) arguments = arguments.Append(value).ToArray();
        var (stdout, stderr) = (Console.Out, Console.Error);
        using var output = new StringWriter();
        try
        {
            Console.SetOut(output);
            Console.SetError(output);
            var serving = Task.Run(() => Invoke(arguments));
            var written = Console.Out;
            while (!serving.IsCompleted && !Written(written, output))
                await Task.Delay(10);
            Assert.False(serving.IsCompleted, output.ToString());

            Assert.Equal(0, Kill(Environment.ProcessId, signal));
            Assert.Equal(0, await serving);
        }
        finally
        {
            Console.SetOut(stdout);
            Console.SetError(stderr);
        }
    }

    private static bool Written(TextWriter synchronized, StringWriter output)
    {
        lock (synchronized) return output.ToString().Contains("listening on", StringComparison.Ordinal);
    }

    public void Dispose() => Directory.Delete(_directory, recursive: true);
}
