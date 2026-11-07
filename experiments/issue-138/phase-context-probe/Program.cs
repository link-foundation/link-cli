using System.Collections.Concurrent;
using System.Diagnostics;
using Foundation.Data.Doublets.Cli.Tests;

if (args.Length > 0)
{
    FiniteExitChild.Phase("ready");
    Console.WriteLine("finite stdout");
    File.WriteAllText(Path.Combine(args[0], "ready"), "ready");
    var gate = Stopwatch.StartNew();
    while (!File.Exists(Path.Combine(args[0], "continue")))
    {
        if (gate.Elapsed > TimeSpan.FromSeconds(5)) return 8;
        Thread.Sleep(10);
    }
    for (int phase = 0; phase < 4; phase++)
    {
        Thread.Sleep(200);
        FiniteExitChild.Phase("operation-complete");
    }
    return 0;
}

var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
info.Environment["DOTNET_GCHeapHardLimit"] = "0x10000000";
info.ArgumentList.Add(typeof(DeferredContext).Assembly.Location);
string directory = Directory.CreateTempSubdirectory("phase-context-probe-").FullName;
info.ArgumentList.Add(directory);
using var child = Process.Start(info)!;
var context = new DeferredContext();
Task? supervision = null;
try
{
    using var readyDeadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
    while (!File.Exists(Path.Combine(directory, "ready"))) await Task.Delay(10, readyDeadline.Token);
    var previous = SynchronizationContext.Current;
    try
    {
        SynchronizationContext.SetSynchronizationContext(context);
        supervision = FiniteExitChild.Wait(child, 0, "deferred synchronization context", TimeSpan.FromMilliseconds(500));
    }
    finally { SynchronizationContext.SetSynchronizationContext(previous); }
    File.WriteAllText(Path.Combine(directory, "continue"), "continue");
    using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
    await child.WaitForExitAsync(deadline.Token);
    Console.WriteLine($"Real finite child exited {child.ExitCode}; queued context callbacks={context.Count}.");
    context.Drain();
    await supervision;
    Console.WriteLine("PASS finite progress supervision does not depend on caller synchronization context.");
    return 0;
}
catch (Exception error)
{
    Console.Error.WriteLine(error);
    return 1;
}
finally
{
    if (!child.HasExited) { child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); }
    context.Drain();
    if (supervision is not null)
        try { await supervision; } catch { /* The original exception was reported above. */ }
    Directory.Delete(directory, recursive: true);
}

internal sealed class DeferredContext : SynchronizationContext
{
    private readonly ConcurrentQueue<(SendOrPostCallback Callback, object? State)> callbacks = new();
    internal int Count => callbacks.Count;
    public override void Post(SendOrPostCallback callback, object? state) => callbacks.Enqueue((callback, state));
    internal void Drain() { while (callbacks.TryDequeue(out var item)) item.Callback(item.State); }
}

namespace Foundation.Data.Doublets.Cli.Tests
{
    // The linked helper uses one xUnit assertion; the standalone probe keeps
    // its semantics without importing a test framework or package.
    internal static class Assert
    {
        internal static void True(bool value, string detail) { if (!value) throw new InvalidOperationException(detail); }
    }
}
