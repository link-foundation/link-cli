using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

public class FiniteExitPoolTests
{
    [Theory(Timeout = 15000)]
    [InlineData("progress")]
    [InlineData("stall")]
    [InlineData("repeat")]
    public async Task SupervisionDoesNotNeedAnAvailableSharedWorker(string mode)
    {
        await FiniteExitChild.WithDirectory("finite-exit-pool-", async directory =>
        {
            var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
            foreach (string arg in new[] { typeof(FiniteExitPoolTests).Assembly.Location, "finite-exit-pool", directory, mode })
                info.ArgumentList.Add(arg);
            await FiniteExitChild.Run(info, 0, $"isolated occupied worker: {mode}");
            File.Delete(Path.Combine(directory, "held"));
        });
    }

    internal static int Child(string directory, string mode)
    {
        // Only this isolated test process changes its worker admission. The
        // actual test runner and all production pool settings stay untouched.
        Assert.True(ThreadPool.SetMinThreads(1, 1));
        Assert.True(ThreadPool.SetMaxThreads(1, 1));
        using var child = FiniteExitChildTests.Start(directory, mode == "progress" ? "deferred-context" : mode);
        using var entered = new ManualResetEventSlim();
        using var release = new ManualResetEventSlim();
        using var finished = new ManualResetEventSlim();
        Task? supervision = null;
        try
        {
            Poll(() => File.Exists(Path.Combine(directory, "ready")), TimeSpan.FromSeconds(5), "child ready");
            ThreadPool.QueueUserWorkItem(_ =>
            {
                entered.Set();
                try { release.Wait(TimeSpan.FromSeconds(5)); }
                finally { finished.Set(); }
            });
            Assert.True(entered.Wait(TimeSpan.FromSeconds(5)), "shared worker was not occupied");
            supervision = FiniteExitChild.Wait(child, 0, $"occupied worker {mode}", TimeSpan.FromMilliseconds(mode == "progress" ? 500 : 50));
            if (mode == "progress") File.WriteAllText(Path.Combine(directory, "continue"), "continue");
            // Do not query HasExited or WaitForExit while waiting: those can
            // raise the process event synchronously and hide the queued race.
            Poll(() => supervision.IsCompleted, TimeSpan.FromSeconds(3), "supervision needed the occupied shared worker");
            if (mode == "progress") supervision.GetAwaiter().GetResult();
            else
            {
                var error = Record.Exception(() => supervision.GetAwaiter().GetResult());
                if (mode == "stall") Assert.IsType<TimeoutException>(error);
                else Assert.Contains("invalid finite progress", Assert.IsType<InvalidOperationException>(error).Message);
                Assert.Contains("finite stdout", error!.Message);
                Assert.Contains("phase=ready", error.Message);
            }
            Assert.True(child.HasExited);
            File.Delete(Path.Combine(directory, "held"));
            Console.WriteLine($"PASS occupied shared worker: {mode}; child reaped before worker release");
            return 0;
        }
        finally
        {
            release.Set();
            if (!child.HasExited) { child.Kill(entireProcessTree: true); child.WaitForExit(); }
            if (supervision is not null)
                try { supervision.GetAwaiter().GetResult(); } catch { /* Preserve the control's primary failure. */ }
            Assert.True(finished.Wait(TimeSpan.FromSeconds(5)), "occupied worker was not released");
        }
    }

    private static void Poll(Func<bool> ready, TimeSpan limit, string description)
    {
        var elapsed = Stopwatch.StartNew();
        while (!ready())
        {
            if (elapsed.Elapsed >= limit) throw new TimeoutException(description);
            Thread.Sleep(10);
        }
    }
}
