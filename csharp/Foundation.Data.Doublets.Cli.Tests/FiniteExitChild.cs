using System.Diagnostics;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Tests;

// Supervision only: phases and elapsed times do not prove persistence.
internal static class FiniteExitChild
{
    private static readonly object phaseGate = new();
    private static readonly Stopwatch phaseElapsed = Stopwatch.StartNew();
    private static int phaseNumber;
    internal static void Phase(string phase)
    {
        lock (phaseGate) Console.Error.WriteLine($"@link-progress:{++phaseNumber} phase={phase} elapsed={phaseElapsed.ElapsedMilliseconds}ms");
    }

    private static async Task<string> Capture(StreamReader reader, FinitePhaseWatch? progress = null)
    {
        const int limit = 65536;
        var text = new StringBuilder();
        var buffer = new char[1024];
        int read;
        bool truncated = false;
        while ((read = await reader.ReadAsync(buffer)) != 0)
        {
            progress?.Observe(buffer.AsSpan(0, read));
            int keep = Math.Min(read, limit - text.Length);
            text.Append(buffer, 0, keep);
            truncated |= keep != read;
        }
        progress?.Complete();
        if (truncated) text.Append("\n[finite diagnostic capture truncated]");
        return text.ToString();
    }

    internal static async Task Run(ProcessStartInfo info, int expectedExit, string description)
    {
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x10000000";
        using var child = Process.Start(info)!;
        await Wait(child, expectedExit, description);
    }

    internal static async Task Wait(Process child, int expectedExit, string description, TimeSpan? limit = null)
    {
        using var progress = new FinitePhaseWatch(limit ?? TimeSpan.FromSeconds(15));
        var stdout = Capture(child.StandardOutput);
        var stderr = Capture(child.StandardError, progress);
        var elapsed = Stopwatch.StartNew();
        Exception? failure = null;
        try { await child.WaitForExitAsync(progress.Token); }
        catch (Exception error) { failure = error; }
        // Reap before either returning or allowing fixture cleanup, including
        // exceptions other than deadline cancellation. Never abandon a child.
        try
        {
            if (!child.HasExited) child.Kill(entireProcessTree: true);
            await child.WaitForExitAsync();
        }
        catch (Exception cleanup)
        {
            throw failure is null ? cleanup : new AggregateException(failure, cleanup);
        }
        string output = await stdout, errors = await stderr;
        string detail = $"{description}; pid={child.Id}; elapsed={elapsed.Elapsed}; phase={progress.LastPhase}; exited {child.ExitCode}\nstdout:\n{output}\nstderr:\n{errors}";
        if (progress.Failure is not null) throw new InvalidOperationException($"{progress.Failure}; {detail}", failure);
        if (failure is OperationCanceledException) throw new TimeoutException(detail, failure);
        if (failure is not null) throw new InvalidOperationException(detail, failure);
        Assert.True(child.ExitCode == expectedExit, detail);
    }

    internal static async Task WithDirectory(string prefix, Func<string, Task> action, Action<string>? cleanup = null)
    {
        string directory = Directory.CreateTempSubdirectory(prefix).FullName;
        Exception? failure = null;
        try { await action(directory); }
        catch (Exception error) { failure = error; throw; }
        finally
        {
            try
            {
                if (cleanup is null) Directory.Delete(directory, recursive: true);
                else cleanup(directory);
            }
            catch (Exception error) when (failure is not null) { throw new AggregateException(failure, error); }
        }
    }
}
