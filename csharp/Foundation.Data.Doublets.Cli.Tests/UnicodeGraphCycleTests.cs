using System.Diagnostics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli.Tests;

public class UnicodeGraphCycleTests
{
    [Theory(Timeout = 30000)]
    [InlineData("string")]
    [InlineData("converter")]
    public async Task CyclicGraphsFailWithoutExhaustingResources(string operation)
    {
        var directory = Directory.CreateTempSubdirectory("unicode-cycle-").FullName;
        var info = new ProcessStartInfo(OperatingSystem.IsLinux() ? "bash" : "dotnet")
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true
        };
        if (OperatingSystem.IsLinux())
        {
            foreach (var argument in new[] { "-c", "ulimit -c 0\nulimit -t 2\nexec \"$@\"", "unicode-cycle-child", "dotnet" }) info.ArgumentList.Add(argument);
        }
        info.ArgumentList.Add(typeof(UnicodeGraphCycleTests).Assembly.Location);
        info.ArgumentList.Add("unicode-cycle");
        info.ArgumentList.Add(operation);
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
                throw new TimeoutException("Finite Unicode graph child failed to stop.");
            }
            var output = await stdout;
            Assert.True(process.ExitCode == 0, $"Unicode graph {operation} exited {process.ExitCode}:\n{output}\n{await stderr}");
            Assert.Contains("Unicode cycles rejected", output);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    internal static int Child(string operation, string directory)
    {
        foreach (var shape in new[] { "source-self", "target-self", "source-pair", "target-pair" })
        {
            var path = Path.Combine(directory, shape + ".links");
            var constants = new LinksConstants<uint>(enableExternalReferencesSupport: true);
            using var memory = new FileMappedResizableDirectMemory(path, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
            using var raw = new UnitedMemoryLinks<uint>(memory, UnitedMemoryLinks<uint>.DefaultLinksSizeStep, constants, Platform.Data.Doublets.Memory.IndexTreeType.Default);
            var links = raw.DecorateWithAutomaticUniquenessAndUsagesRepointing();
            var storage = new UnicodeStringStorage<uint>(links);
            var symbol = links.GetSource(links.GetTarget(storage.CreateString("x")));
            var a = links.Create(); var b = links.Create();
            switch (shape)
            {
                case "source-self": links.Update(a, a, symbol); break;
                case "target-self": links.Update(a, symbol, a); break;
                case "source-pair": links.Update(a, b, symbol); links.Update(b, a, symbol); break;
                case "target-pair": links.Update(a, symbol, b); links.Update(b, symbol, a); break;
            }
            var sequence = links.GetOrCreate(a, storage.UnicodeSequenceType);
            var value = links.GetOrCreate(storage.StringType, sequence);
            for (int repeat = 0; repeat < 2; repeat++)
            {
                var error = Assert.Throws<InvalidDataException>(() => operation == "string"
                    ? storage.GetString(value) : storage.UnicodeSequenceToStringConverter.Convert(sequence));
                Assert.Contains("cycle", error.Message);
            }
            Assert.Equal("A😀雪A", storage.GetString(storage.CreateString("A😀雪A")));
        }
        Console.WriteLine("Unicode cycles rejected");
        return 0;
    }
}
