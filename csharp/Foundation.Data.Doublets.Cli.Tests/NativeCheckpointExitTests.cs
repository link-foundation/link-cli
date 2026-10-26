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
    internal static int Child(string path, bool mirrored, int point, bool repair)
    {
        var trace = new List<string>(); bool active = repair;
        void Observe(string name, string operation, bool after)
        {
            if (!active) return;
            trace.Add($"{name[path.Length..]}:{operation}:{after}"); if (trace.Count - 1 == point) Environment.Exit(73);
        }
        FilePageDevice Open(string name) => new(name, (file, owner) => new ObservedFile(file, owner, Observe), _ => { });
        using var log = new FileCheckpointPagedCommitLog(path, mirrored, 64, Open);
        if (!repair) { active = true; log.Replace(Replacement); }
        active = false; log.Dispose(); Console.WriteLine(JsonSerializer.Serialize(trace)); return 0;
    }
    private static void Seed(string path, bool mirrored, bool repair)
    {
        using var log = new FileCheckpointPagedCommitLog(path, mirrored, 64); log.Replace(new[] { "retired bank" }); log.Replace(new[] { "acknowledged" }); log.Dispose();
        if (repair) { File.WriteAllBytes(path + ".bank0", File.ReadAllBytes(path + ".bank0")[..19]); File.WriteAllText(path + ".bank0.meta0", "torn"); }
    }
    private static async Task<string> RunChild(string path, bool mirrored, int point, bool repair)
    {
        var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x06000000";
        foreach (string argument in new[] { typeof(NativeCheckpointExitTests).Assembly.Location, "checkpoint-pages-cut", path, mirrored.ToString(), point.ToString(System.Globalization.CultureInfo.InvariantCulture), repair.ToString() }) info.ArgumentList.Add(argument);
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

    [Theory(Timeout = 120000)]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task EveryRealNativeMutationSurvivesBeforeAndAfterProcessExit(bool mirrored, bool repair)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-cut-").FullName;
        try
        {
            string baseline = Path.Combine(directory, "baseline"); Seed(baseline, mirrored, repair);
            string[] points = JsonSerializer.Deserialize<string[]>(await RunChild(baseline, mirrored, -1, repair))!; Assert.InRange(points.Length, repair ? 12 : 30, 150);
            foreach (string operation in new[] { "write", "truncate", "sync" }) Assert.Contains(points, value => value.Contains(operation, StringComparison.Ordinal));
            Assert.Contains(points, value => value.StartsWith(repair ? ".bank0:write:" : ".meta1:write:", StringComparison.Ordinal));
            for (int point = 0; point < points.Length; point++)
            {
                string path = Path.Combine(directory, $"case-{point}"); Seed(path, mirrored, repair); await RunChild(path, mirrored, point, repair);
                Assert.Throws<IOException>(() => new FileCheckpointPagedCommitLog(path, mirrored));
                // The observed child exit proves its ownership is now fenced.
                foreach (string suffix in mirrored ? new[] { "", ".bank0", ".bank0.mirror", ".bank1", ".bank1.mirror" } : new[] { "", ".bank0", ".bank1" }) File.Delete(path + suffix + ".writer");
                using var recovered = new FileCheckpointPagedCommitLog(path, mirrored, 64); var actual = recovered.Read();
                Assert.True(actual.SequenceEqual(new[] { "acknowledged" }) || actual.SequenceEqual(Replacement), $"torn result at point {point}: {points[point]}");
                if (repair) Assert.Equal(new[] { "acknowledged" }, actual);
                recovered.Append("following"); recovered.Flush(); Assert.Equal(actual.Append("following"), recovered.Read());
                Assert.True(recovered.Replace(new[] { "compacted after exit" })); recovered.Dispose();
                using var again = new FileCheckpointPagedCommitLog(path, mirrored); Assert.Equal(new[] { "compacted after exit" }, again.Read());
            }
            Console.WriteLine($"native checkpoint mirrored={mirrored} repair={repair}: {points.Length} bounded before/after process exits");
        }
        finally { Directory.Delete(directory, true); }
    }
}
