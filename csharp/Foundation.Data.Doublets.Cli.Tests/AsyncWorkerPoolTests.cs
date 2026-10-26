using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AsyncWorkerPoolTests
{
    [Theory(Timeout = 30000)]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public async Task IdleSynchronousOwnersLeaveWorkersAvailableForAsyncCheckpoints(bool blockingControl, bool schedulingControl)
    {
        var info = new ProcessStartInfo("dotnet")
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        info.ArgumentList.Add(typeof(AsyncWorkerPoolTests).Assembly.Location);
        info.ArgumentList.Add("async-worker-pool");
        if (blockingControl) info.ArgumentList.Add("blocking-control");
        else if (schedulingControl) info.ArgumentList.Add("scheduling-control");
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x6000000";
        using var process = Process.Start(info)!;
        var stdout = process.StandardOutput.ReadToEndAsync();
        var stderr = process.StandardError.ReadToEndAsync();
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
        try
        {
            await process.WaitForExitAsync(deadline.Token);
        }
        catch (OperationCanceledException)
        {
            process.Kill(entireProcessTree: true);
            await process.WaitForExitAsync();
            throw new TimeoutException("Finite worker-pool child failed to stop.");
        }
        string output = await stdout, errors = await stderr;
        Assert.True(process.ExitCode == (blockingControl ? 1 : 0), $"Finite worker-pool child exited {process.ExitCode}:\n{output}\n{errors}");
        Assert.Contains(blockingControl ? "responsive=False, checkpoint=False" : "responsive=True, checkpoint=True", output);
    }
}
