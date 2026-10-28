using System.Diagnostics;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeCheckpointExitTests
{
    private sealed class ObservedFile(string path, bool owner, Action<string, string, bool> observe)
        : FileStream(path, owner ? FileMode.CreateNew : FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read, bufferSize: 1)
    {
        public override void Write(ReadOnlySpan<byte> bytes) { observe(Name, "write", false); base.Write(bytes); observe(Name, "write", true); }
        public override void SetLength(long length) { observe(Name, "truncate", false); base.SetLength(length); observe(Name, "truncate", true); }
        public override void Flush(bool flushToDisk)
        {
            if (flushToDisk) observe(Name, "sync", false); base.Flush(flushToDisk); if (flushToDisk) observe(Name, "sync", true);
        }
    }
    private static string[] Replacement => new[] { new string('雪', 700), "\ufeff\0😀" };
    internal static int Child(string path, bool mirrored, int point, string phase, bool mirroredRoot)
    {
        var trace = new List<string>(); bool active = phase != "replace";
        void Observe(string name, string operation, bool after)
        {
            if (!active) return;
            trace.Add($"{name[path.Length..]}:{operation}:{after}"); if (trace.Count - 1 == point) Environment.Exit(73);
        }
        FilePageDevice Open(string name) => new(name, (file, owner) => new ObservedFile(file, owner, Observe), _ => { });
        using var log = new FileCheckpointPagedCommitLog(path, mirrored, 64, Open, mirroredRoot);
        if (phase == "replace") { active = true; log.Replace(Replacement); }
        active = false; log.Dispose(); Console.WriteLine(JsonSerializer.Serialize(trace)); return 0;
    }
    private static void Seed(string path, bool mirrored, bool mirroredRoot, string phase)
    {
        using var log = new FileCheckpointPagedCommitLog(path, mirrored, mirroredRoot, 64); log.Replace(new[] { "retired bank" }); log.Replace(new[] { "acknowledged" }); log.Dispose();
        if (phase is "root-repair" or "both-repair") File.WriteAllText(path + ".meta0", "torn root");
        if (phase is "repair" or "both-repair") { File.WriteAllBytes(path + ".bank0", File.ReadAllBytes(path + ".bank0")[..19]); File.WriteAllText(path + ".bank0.meta0", "torn"); }
    }
    private static async Task<string> RunChild(string path, bool mirrored, int point, string phase, bool mirroredRoot)
    {
        var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x06000000";
        foreach (string argument in new[] { typeof(NativeCheckpointExitTests).Assembly.Location, "checkpoint-pages-cut", path, mirrored.ToString(), point.ToString(System.Globalization.CultureInfo.InvariantCulture), phase, mirroredRoot.ToString() }) info.ArgumentList.Add(argument);
        using var child = Process.Start(info)!; var stdout = child.StandardOutput.ReadToEndAsync(); var stderr = child.StandardError.ReadToEndAsync();
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        try { await child.WaitForExitAsync(deadline.Token); }
        catch (OperationCanceledException)
        {
            child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); await Task.WhenAll(stdout, stderr); throw new TimeoutException("Finite checkpoint page child expired.");
        }
        string output = await stdout, error = await stderr; Assert.InRange(output.Length + error.Length, 0, 65536);
        Assert.True(child.ExitCode == (point < 0 ? 0 : 73), $"point {point}: exit {child.ExitCode}: {error}"); return output;
    }

    // Keep every observed cut while bounding each xUnit case independently.
    // The full mirrored/root matrix exceeded 120 seconds in Windows CI;
    // slicing bounds each case's process and synchronized-file work.
    private const int Batches = 4;
    public static IEnumerable<object[]> Cases()
    {
        var configurations = new[] {
            (false,false,"replace"),(true,false,"replace"),(true,false,"repair"),
            (false,true,"replace"),(true,true,"replace"),(true,true,"repair"),
            (false,true,"root-repair"),(true,true,"root-repair"),(true,true,"both-repair") };
        foreach (var (mirrored, root, phase) in configurations)
            for (int batch = 0; batch < Batches; batch++) yield return new object[] { mirrored, root, phase, batch };
    }
    [Theory(Timeout = 120000)]
    [MemberData(nameof(Cases))]
    public async Task EveryRealNativeMutationSurvivesBeforeAndAfterProcessExit(bool mirrored, bool mirroredRoot, string phase, int batch)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-cut-").FullName;
        try
        {
            string baseline = Path.Combine(directory, "baseline"); Seed(baseline, mirrored, mirroredRoot, phase);
            string[] points = JsonSerializer.Deserialize<string[]>(await RunChild(baseline, mirrored, -1, phase, mirroredRoot))!; Assert.InRange(points.Length, phase == "replace" ? 30 : phase == "root-repair" ? 6 : 12, 150);
            foreach (string operation in new[] { "write", "truncate", "sync" }) Assert.Contains(points, value => value.Contains(operation, StringComparison.Ordinal));
            Assert.Contains(points, value => value.StartsWith(phase == "replace" ? ".meta1:write:" : phase == "root-repair" ? ".meta0:write:" : ".bank0:write:", StringComparison.Ordinal));
            for (int point = batch; point < points.Length; point += Batches)
            {
                string path = Path.Combine(directory, $"case-{point}"); Seed(path, mirrored, mirroredRoot, phase); await RunChild(path, mirrored, point, phase, mirroredRoot);
                Assert.Throws<IOException>(() => new FileCheckpointPagedCommitLog(path, mirrored, mirroredRoot));
                if (phase == "both-repair") Assert.Contains(points, value => value.StartsWith(".meta0:write:", StringComparison.Ordinal));
                // The observed child exit proves its ownership is now fenced.
                foreach (string suffix in new[] { "" }.Concat(mirroredRoot ? new[] { ".mirror" } : Array.Empty<string>()).Concat(mirrored ? new[] { ".bank0", ".bank0.mirror", ".bank1", ".bank1.mirror" } : new[] { ".bank0", ".bank1" })) File.Delete(path + suffix + ".writer");
                using var recovered = new FileCheckpointPagedCommitLog(path, mirrored, mirroredRoot, 64); var actual = recovered.Read();
                Assert.True(actual.SequenceEqual(new[] { "acknowledged" }) || actual.SequenceEqual(Replacement), $"torn result at point {point}: {points[point]}");
                if (phase != "replace") Assert.Equal(new[] { "acknowledged" }, actual);
                recovered.Append("following"); recovered.Flush(); Assert.Equal(actual.Append("following"), recovered.Read());
                Assert.True(recovered.Replace(new[] { "compacted after exit" })); recovered.Dispose();
                using var again = new FileCheckpointPagedCommitLog(path, mirrored, mirroredRoot); Assert.Equal(new[] { "compacted after exit" }, again.Read());
            }
            Console.WriteLine($"native checkpoint mirroredBanks={mirrored} mirroredRoot={mirroredRoot} phase={phase} batch={batch}/{Batches}: {points.Length} total points; {(points.Length + Batches - 1 - batch) / Batches} bounded before/after process exits");
        }
        finally { Directory.Delete(directory, true); }
    }
}
