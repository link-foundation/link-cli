using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Runs the <c>clink</c> executable built next to the tests.</summary>
public static class Clink
{
    private static readonly string Executable = Path.Combine(AppContext.BaseDirectory, "clink.dll");

    public sealed record Result(int ExitCode, string Stdout, string Stderr);

    public static ProcessStartInfo StartInfo(IEnumerable<string> arguments)
    {
        var info = new ProcessStartInfo("dotnet")
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        info.ArgumentList.Add(Executable);
        foreach (var argument in arguments)
        {
            info.ArgumentList.Add(argument);
        }
        return info;
    }

    public static async Task<Result> RunAsync(params string[] arguments)
    {
        using var process = Process.Start(StartInfo(arguments))!;
        var stdout = process.StandardOutput.ReadToEndAsync();
        var stderr = process.StandardError.ReadToEndAsync();
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
        try
        {
            await process.WaitForExitAsync(timeout.Token);
        }
        catch (OperationCanceledException)
        {
            process.Kill(entireProcessTree: true);
            throw new TimeoutException($"clink {string.Join(' ', arguments)} did not exit within a minute.");
        }
        return new Result(process.ExitCode, (await stdout).Replace("\r\n", "\n"), (await stderr).Replace("\r\n", "\n"));
    }

    public static void AssertSucceeded(Result result) =>
        Assert.True(
            result.ExitCode == 0,
            $"clink exited with {result.ExitCode}\nstdout:\n{result.Stdout}\nstderr:\n{result.Stderr}");
}
