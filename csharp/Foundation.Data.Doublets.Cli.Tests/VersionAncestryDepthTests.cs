using System.Diagnostics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionAncestryDepthTests
{
    // Separate name storage isolates traversal from Unicode name construction.
    // This fixture claims no persistence guarantee; semantic tests use real names.
    private sealed class FlatMetadata(string path) : NamedTypesDecorator<uint>(path)
    {
        private readonly Dictionary<uint, string> _names = new();
        public override string? GetName(uint link) => _names.GetValueOrDefault(link);
        public override uint SetName(uint link, string name) { _names[link] = name; return link; }
        public override void RemoveName(uint link) => _names.Remove(link);
    }

    [Theory(Timeout = 30000)]
    [InlineData("checkout")]
    [InlineData("switch")]
    [InlineData("fork")]
    public async Task ValidDeepAncestryUsesBoundedStack(string operation)
    {
        var directory = Directory.CreateTempSubdirectory("finite-ancestry-").FullName;
        var info = new ProcessStartInfo(OperatingSystem.IsLinux() ? "bash" : "dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
        if (OperatingSystem.IsLinux())
        {
            foreach (var argument in new[] { "-c", "ulimit -c 0\nexec \"$@\"", "ancestry-child", "dotnet" }) info.ArgumentList.Add(argument);
        }
        info.ArgumentList.Add(typeof(VersionAncestryDepthTests).Assembly.Location);
        info.ArgumentList.Add("deep-ancestry");
        info.ArgumentList.Add(operation);
        info.ArgumentList.Add(directory);
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x10000000";
        info.Environment["DOTNET_DbgEnableMiniDump"] = "0";
        info.Environment["COMPlus_DbgEnableMiniDump"] = "0";
        try
        {
            using var process = Process.Start(info)!;
            var stdout = process.StandardOutput.ReadToEndAsync();
            var stderr = process.StandardError.ReadToEndAsync();
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            try { await process.WaitForExitAsync(deadline.Token); }
            catch (OperationCanceledException)
            {
                process.Kill(entireProcessTree: true);
                await process.WaitForExitAsync();
                throw new TimeoutException("Finite ancestry child failed to stop.");
            }
            var output = await stdout;
            Assert.True(process.ExitCode == 0, $"Finite child {operation} exited {process.ExitCode}:\n{output}\n{await stderr}");
            Assert.Contains("Finite ancestry contract passed", output);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    internal static int Child(string operation, string directory)
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            try { Contract(operation, directory); }
            catch (Exception error) { failure = error; }
        }, 256 * 1024);
        thread.Start();
        thread.Join();
        if (failure is not null) throw failure;
        return 0;
    }

    private static void Contract(string operation, string directory)
    {
        using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
        using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
        using var branches = new FlatMetadata(Path.Combine(directory, "branches.links"));
        using var tx = new TransactionsDecorator(data, log);
        const int depth = 2048;
        void Marker(string text) => branches.SetName(branches.CreateAndUpdate(0u, 0u), text);
        Marker("__vc:branch:main:parent=:fork=0:head=0");
        for (var index = 1; index <= depth; ++index)
        {
            var parent = index == 1 ? "main" : $"branch-{index - 1}";
            Marker($"__vc:branch:branch-{index}:parent={parent}:fork=0:head=0");
        }
        Marker($"__vc:current=branch-{depth}");
        Marker("__vc:applied=0");
        using var version = new VersionControlDecorator(tx, branches);
        Console.WriteLine($"Constructed {version.ListBranches().Count} valid branches; {operation}");
        switch (operation)
        {
            case "checkout": version.Checkout(0); break;
            case "switch": version.SwitchBranch("main"); version.SwitchBranch($"branch-{depth}"); break;
            case "fork":
                Assert.Throws<InvalidOperationException>(() => version.Branch("following", 1));
                version.Branch("following", 0);
                break;
            default: throw new InvalidOperationException("Unknown finite ancestry operation.");
        }
        Assert.Equal($"branch-{depth}", version.CurrentBranch);
        Assert.Equal(0, version.CurrentSequence);
        Assert.Equal(depth + 1 + (operation == "fork" ? 1 : 0), version.ListBranches().Count);
        var id = version.CreateAndUpdate(11u, 12u);
        var sequence = version.CurrentSequence;
        Assert.True(sequence > 0);
        version.Checkout(0); version.Checkout(sequence);
        Assert.Equal(11u, version.GetSource(id));
        Console.WriteLine($"Finite ancestry contract passed: {operation}");
    }
}
