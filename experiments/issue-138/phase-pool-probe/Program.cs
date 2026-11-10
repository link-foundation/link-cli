using System.Diagnostics;
using Foundation.Data.Doublets.Cli.Tests;

if (args is ["timer"])
{
    ThreadPool.SetMinThreads(1, 1); ThreadPool.SetMaxThreads(1, 1);
    using var entered = new ManualResetEventSlim();
    using var release = new ManualResetEventSlim();
    using var finished = new ManualResetEventSlim();
    ThreadPool.QueueUserWorkItem(_ => { entered.Set(); release.Wait(TimeSpan.FromSeconds(5)); finished.Set(); });
    if (!entered.Wait(TimeSpan.FromSeconds(5))) throw new TimeoutException("timer worker");
    using var watch = new FinitePhaseWatch(TimeSpan.FromMilliseconds(500));
    Thread.Sleep(600);
    Console.WriteLine($"After due timer, before release: cancelled={watch.Token.IsCancellationRequested}");
    watch.Observe("@link-progress:1 phase=ready\n");
    release.Set();
    finished.Wait(TimeSpan.FromSeconds(5));
    Thread.Sleep(100);
    Console.WriteLine($"100ms after fresh phase: cancelled={watch.Token.IsCancellationRequested}");
    return watch.Token.IsCancellationRequested ? 1 : 0;
}
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
    File.WriteAllText(Path.Combine(args[0], "done"), "done");
    return 0;
}

if (!ThreadPool.SetMinThreads(1, 1) || !ThreadPool.SetMaxThreads(1, 1))
    throw new InvalidOperationException("Could not bound the isolated process thread pool.");
string directory = Directory.CreateTempSubdirectory("phase-pool-probe-").FullName;
var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
info.Environment["DOTNET_GCHeapHardLimit"] = "0x10000000";
info.ArgumentList.Add(typeof(Assert).Assembly.Location);
info.ArgumentList.Add(directory);
using var child = Process.Start(info)!;
using var workerEntered = new ManualResetEventSlim();
using var releaseWorker = new ManualResetEventSlim();
Task? supervision = null;
try
{
    var ready = Stopwatch.StartNew();
    while (!File.Exists(Path.Combine(directory, "ready")))
    {
        if (ready.Elapsed > TimeSpan.FromSeconds(5)) throw new TimeoutException("ready");
        Thread.Sleep(10);
    }
    ThreadPool.QueueUserWorkItem(_ => { workerEntered.Set(); releaseWorker.Wait(TimeSpan.FromSeconds(5)); });
    if (!workerEntered.Wait(TimeSpan.FromSeconds(5))) throw new TimeoutException("worker");
    supervision = FiniteExitChild.Wait(child, 0, "bounded shared pool control", TimeSpan.FromMilliseconds(500));
    File.WriteAllText(Path.Combine(directory, "continue"), "continue");
    var completion = Stopwatch.StartNew();
    while (!File.Exists(Path.Combine(directory, "done")))
    {
        if (completion.Elapsed > TimeSpan.FromSeconds(5)) throw new TimeoutException("child work");
        Thread.Sleep(10);
    }
    // Querying HasExited/WaitForExit here would synchronously raise the process
    // event on this thread and hide the queued-exit/queued-timer ordering.
    Thread.Sleep(100);
    Console.WriteLine("Finite child completed all phases; shared worker still deliberately occupied.");
    releaseWorker.Set();
    supervision.GetAwaiter().GetResult();
    Console.WriteLine("PASS finite progress supervision survives a bounded occupied shared pool.");
    return 0;
}
catch (Exception error) { Console.Error.WriteLine(error); return 1; }
finally
{
    releaseWorker.Set();
    if (!child.HasExited) { child.Kill(entireProcessTree: true); child.WaitForExit(); }
    if (supervision is not null) try { supervision.GetAwaiter().GetResult(); } catch { }
    Directory.Delete(directory, recursive: true);
}
namespace Foundation.Data.Doublets.Cli.Tests
{
    internal static class Assert
    {
        internal static void True(bool value, string detail) { if (!value) throw new InvalidOperationException(detail); }
    }
}
