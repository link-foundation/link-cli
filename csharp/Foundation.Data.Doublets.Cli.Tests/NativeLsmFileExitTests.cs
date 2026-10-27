using System.Diagnostics;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeLsmFileExitTests
{
    private static readonly string[] Phases = { "open", "group", "flush", "following", "compact" };
    private static FileLsmOptions Options(bool mirrored) => new(PhysicalLsmHarness.Options(), 128, mirrored, mirrored, mirrored);
    internal static int Child(string directory, string stop, bool mirrored)
    {
        if (!Phases.Contains(stop)) throw new ArgumentException("Finite native LSM phase.", nameof(stop));
        var store = FileLsmJournalStore.Open(directory, Options(mirrored));
        void Phase(string phase) { Console.WriteLine(phase); if (phase == stop) Environment.Exit(73); }
        Phase("open");
        if (!store.CompareExchange(new[] { "a", "b" }, store.Read(new[] { "a", "b" }), new IReadOnlyList<string>?[] { new[] { "durable\0雪😀" }, Array.Empty<string>() })) throw new InvalidOperationException("Rejected group.");
        Phase("group"); store.Flush(); Phase("flush");
        if (!store.CompareExchange(new[] { "a" }, store.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "following" } })) throw new InvalidOperationException("Rejected following group.");
        Phase("following"); store.Compact(); Phase("compact"); store.Dispose(); return 2;
    }
    private static async Task<string> RunChild(string directory, string phase, bool mirrored)
    {
        var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x06000000";
        foreach (string argument in new[] { typeof(ProbeProgram).Assembly.Location, "native-lsm-exit", directory, phase, mirrored.ToString() }) info.ArgumentList.Add(argument);
        using var child = Process.Start(info)!;
        var stdout = child.StandardOutput.ReadToEndAsync(); var stderr = child.StandardError.ReadToEndAsync();
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(30));
        try { await child.WaitForExitAsync(deadline.Token); }
        catch (OperationCanceledException)
        { child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); await Task.WhenAll(stdout, stderr); throw new TimeoutException("Finite native LSM child expired."); }
        string output = await stdout, error = await stderr;
        Assert.InRange(output.Length + error.Length, 0, 4096); Assert.True(child.ExitCode == 73, error); return output;
    }
    [Theory(Timeout = 120000)]
    [InlineData(false)]
    [InlineData(true)]
    public async Task FiveReapedAbruptChildrenRetainAcknowledgedGroupsAndExclusiveOwners(bool mirrored)
    {
        foreach (string phase in Phases)
        {
            string directory = Directory.CreateTempSubdirectory("lsm-native-exit-").FullName;
            try
            {
                string output = await RunChild(directory, phase, mirrored);
                Assert.Equal(Phases.Take(Array.IndexOf(Phases, phase) + 1), output.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries));
                Assert.Throws<IOException>(() => FileLsmJournalStore.Open(directory, Options(mirrored)));
                // The child has exited and been reaped before external owner recovery.
                foreach (string path in Directory.GetFiles(directory)) if (Path.GetFileName(path) == ".lsm-writer" || path.EndsWith(".writer", StringComparison.Ordinal)) File.Delete(path);
                using (var store = FileLsmJournalStore.Open(directory, Options(mirrored)))
                {
                    var values = store.Read(new[] { "a", "b" });
                    Assert.Equal(phase == "open" ? Array.Empty<string>() : new[] { phase is "following" or "compact" ? "following" : "durable\0雪😀" }, values[0].Entries); Assert.Empty(values[1].Entries);
                    if (phase != "open") Assert.Equal(1UL, values[1].Generation);
                    Assert.True(store.CompareExchange(new[] { "a" }, values.Take(1).ToArray(), new IReadOnlyList<string>?[] { new[] { "after restart" } })); store.Flush();
                }
                using var again = FileLsmJournalStore.Open(directory, Options(mirrored)); Assert.Equal("after restart", again.Read(new[] { "a" })[0].Entries[0]);
            }
            finally { Directory.Delete(directory, true); }
        }
    }
}
