using System.Diagnostics;
using System.Text;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class StructureDepthTests
{
    [Fact(Timeout = 30000)]
    public async Task ValidDeepStructureUsesBoundedStack()
    {
        var directory = Directory.CreateTempSubdirectory("finite-structure-").FullName;
        var info = new ProcessStartInfo(OperatingSystem.IsLinux() ? "bash" : "dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
        if (OperatingSystem.IsLinux())
        {
            foreach (var argument in new[] { "-c", "ulimit -c 0\nulimit -t 4\nexec \"$@\"", "structure-child", "dotnet" }) info.ArgumentList.Add(argument);
        }
        info.ArgumentList.Add(typeof(StructureDepthTests).Assembly.Location);
        info.ArgumentList.Add("deep-structure");
        info.ArgumentList.Add(directory);
        info.Environment["DOTNET_GCHeapHardLimit"] = "0x08000000";
        info.Environment["DOTNET_DbgEnableMiniDump"] = "0";
        info.Environment["COMPlus_DbgEnableMiniDump"] = "0";
        try
        {
            using var process = Process.Start(info)!;
            var stdout = process.StandardOutput.ReadToEndAsync();
            var stderr = process.StandardError.ReadToEndAsync();
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(10));
            try { await process.WaitForExitAsync(deadline.Token); }
            catch (OperationCanceledException)
            {
                process.Kill(entireProcessTree: true);
                await process.WaitForExitAsync();
                throw new TimeoutException("Finite structure child failed to stop.");
            }
            var output = await stdout;
            var error = await stderr;
            Assert.True(process.ExitCode == 0, $"Finite structure exited {process.ExitCode}:\n{output}\n{error[..Math.Min(error.Length, 8192)]}");
            Assert.Contains("Finite structure contract passed", output);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    internal static int Child(string directory)
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            try
            {
                using var links = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                var ids = new List<uint>();
                uint root = 0;
                for (int index = 0; index < 2048; index++)
                {
                    root = links.GetOrCreate(root, 0u);
                    ids.Add(root);
                }
                Console.WriteLine("Prepared finite 2048-node structure");
                var output = LinoDatabaseOutput.FormatStructure(links, root);
                var expected = new StringBuilder();
                foreach (var id in ids.AsEnumerable().Reverse()) expected.Append('(').Append(id).Append(": ");
                expected.Append('0');
                foreach (var _ in ids) expected.Append(" 0)");
                Assert.Equal(expected.ToString(), output);
                Console.WriteLine("Finite structure contract passed");
            }
            catch (Exception error) { failure = error; }
        }, 256 * 1024);
        thread.Start();
        thread.Join();
        if (failure is not null) throw failure;
        return 0;
    }
}
