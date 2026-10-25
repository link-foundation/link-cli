using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AsyncWorkerPoolTests
{
    [Fact(Timeout = 30000)]
    public async Task IdleSynchronousOwnersLeaveWorkersAvailableForAsyncCheckpoints()
    {
        var info = new ProcessStartInfo("dotnet")
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        info.ArgumentList.Add(typeof(AsyncWorkerPoolTests).Assembly.Location);
        info.ArgumentList.Add("async-worker-pool");
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
        Assert.True(process.ExitCode == 0, $"Finite worker-pool child exited {process.ExitCode}:\n{await stdout}\n{await stderr}");
    }
}
