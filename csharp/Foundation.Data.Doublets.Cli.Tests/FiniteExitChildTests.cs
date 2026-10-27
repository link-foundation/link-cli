using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

public class FiniteExitChildTests
{
    internal static int Child(string directory, string mode)
    {
        using var file = new FileStream(Path.Combine(directory, "held"), FileMode.Create, FileAccess.Write, FileShare.None);
        FiniteExitChild.Phase("ready");
        Console.Out.WriteLine("finite stdout");
        File.WriteAllText(Path.Combine(directory, "ready"), "ready");
        if (mode == "fail") return 9;
        if (mode == "progress")
        {
            // More total work than the parent's idle bound, but each distinct
            // operation completes inside it. This is a finite control.
            for (int phase = 0; phase < 4; phase++)
            {
                Thread.Sleep(200);
                FiniteExitChild.Phase("operation-complete");
            }
            return 0;
        }
        if (mode == "repeat") Console.Error.WriteLine("@link-progress:1 phase=repeat");
        // Finite fallback also bounds the probe if its supervisor fails.
        Thread.Sleep(TimeSpan.FromSeconds(10));
        return 0;
    }

    [Fact(Timeout = 15000)]
    public async Task DistinctFiniteProgressSurvivesAnAggregateDeadline()
    {
        await FiniteExitChild.WithDirectory("finite-exit-progress-", async directory =>
        {
            using var child = Start(directory, "progress");
            try
            {
                using var readyDeadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                while (!File.Exists(Path.Combine(directory, "ready"))) await Task.Delay(10, readyDeadline.Token);
                await FiniteExitChild.Wait(child, 0, "progress control", TimeSpan.FromMilliseconds(500));
                Assert.True(child.HasExited);
                File.Delete(Path.Combine(directory, "held"));
            }
            finally
            {
                if (!child.HasExited) { child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); }
            }
        });
    }

    [Fact(Timeout = 15000)]
    public async Task RepeatedProgressFailsAndReapsBeforeDirectoryCleanup()
    {
        await FiniteExitChild.WithDirectory("finite-exit-repeat-", async directory =>
        {
            using var child = Start(directory, "repeat");
            var error = await Assert.ThrowsAsync<InvalidOperationException>(() => FiniteExitChild.Wait(child, 0, "repeat control"));
            Assert.Contains("invalid finite progress", error.Message);
            Assert.Contains("finite stdout", error.Message);
            Assert.True(child.HasExited);
            File.Delete(Path.Combine(directory, "held"));
        });
    }

    private static Process Start(string directory, string mode)
    {
        var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x10000000";
        foreach (string arg in new[] { typeof(FiniteExitChildTests).Assembly.Location, "finite-exit-control", directory, mode }) info.ArgumentList.Add(arg);
        return Process.Start(info)!;
    }

    [Fact(Timeout = 15000)]
    public async Task DeadlineKeepsBothStreamsAndReapsBeforeDirectoryCleanup()
    {
        await FiniteExitChild.WithDirectory("finite-exit-timeout-", async directory =>
        {
            using var child = Start(directory, "stall");
            try
            {
                using var readyDeadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                while (!File.Exists(Path.Combine(directory, "ready"))) await Task.Delay(10, readyDeadline.Token);
                var error = await Assert.ThrowsAsync<TimeoutException>(() => FiniteExitChild.Wait(child, 0, "control", TimeSpan.FromMilliseconds(20)));
                Assert.Contains("phase=ready", error.Message);
                Assert.Contains("finite stdout", error.Message);
                Assert.True(child.HasExited);
                // Windows rejects this while the actual child still owns it.
                File.Delete(Path.Combine(directory, "held"));
            }
            finally
            {
                if (!child.HasExited) { child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); }
            }
        });
    }

    [Fact(Timeout = 15000)]
    public async Task UnexpectedExitKeepsTheActualExitCodeAndBothStreams()
    {
        await FiniteExitChild.WithDirectory("finite-exit-error-", async directory =>
        {
            using var child = Start(directory, "fail");
            var error = await Assert.ThrowsAnyAsync<Exception>(() => FiniteExitChild.Wait(child, 0, "control"));
            Assert.Contains("exited 9", error.Message);
            Assert.Contains("phase=ready", error.Message);
            Assert.Contains("finite stdout", error.Message);
            Assert.True(child.HasExited);
        });
    }

    [Fact]
    public async Task CleanupFailurePreservesThePrimaryFailure()
    {
        var primary = new IOException("actual child failure");
        var cleanup = new IOException("directory cleanup failure");
        string? created = null;
        var error = await Assert.ThrowsAsync<AggregateException>(() => FiniteExitChild.WithDirectory("finite-exit-cleanup-", directory =>
        {
            created = directory;
            return Task.FromException(primary);
        }, _ => throw cleanup));
        Assert.Same(primary, error.InnerExceptions[0]);
        Assert.Same(cleanup, error.InnerExceptions[1]);
        Directory.Delete(created!);
    }
}
