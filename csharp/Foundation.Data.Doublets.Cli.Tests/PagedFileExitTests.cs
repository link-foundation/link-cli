using System.Diagnostics;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class PagedFileExitTests
{
    internal static int Child(string path, string point)
    {
        using var device = new FilePageDevice(path); var log = new PagedCommitLog(device); log.Append("\ufeffuncertain 雪\0😀");
        if (point == "flush") log.Flush();
        Environment.Exit(73); // Deliberately skip disposal; the OS remains live.
        return 0;
    }
    [Theory(Timeout = 30000)]
    [InlineData("append")]
    [InlineData("flush")]
    public async Task BoundedProcessLossPreservesAcknowledgementsAndRequiresExplicitFencing(string point)
    {
        var directory = Directory.CreateTempSubdirectory("page-exit-").FullName; var path = Path.Combine(directory, "journal");
        try
        {
            using (var device = new FilePageDevice(path)) { var log = new PagedCommitLog(device); log.Append("committed"); log.Flush(); }
            var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
            info.Environment["DOTNET_GCHeapHardLimit"] = "0x08000000";
            foreach (string argument in new[] { typeof(PagedFileExitTests).Assembly.Location, "pages-exit", path, point }) info.ArgumentList.Add(argument);
            using var child = Process.Start(info)!; var stdout = child.StandardOutput.ReadToEndAsync(); var stderr = child.StandardError.ReadToEndAsync();
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(10));
            try { await child.WaitForExitAsync(deadline.Token); }
            catch (OperationCanceledException)
            {
                child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); await Task.WhenAll(stdout, stderr);
                throw new TimeoutException("Finite native page child expired.");
            }
            Assert.Equal(73, child.ExitCode); Assert.InRange((await stdout).Length + (await stderr).Length, 0, 65536);
            Assert.ThrowsAny<IOException>(() => new FilePageDevice(path));
            File.Delete(path + ".writer"); // This child is now known dead; never infer this from lock age.
            using (var device = new FilePageDevice(path))
            {
                var log = new PagedCommitLog(device); var expected = point == "flush" ? new[] { "committed", "\ufeffuncertain 雪\0😀" } : new[] { "committed" };
                Assert.Equal(expected, log.Read()); log.Append("following"); log.Flush(); Assert.Equal(expected.Append("following"), log.Read());
            }
            using var reopened = new FilePageDevice(path); Assert.Equal("following", new PagedCommitLog(reopened).Read()[^1]);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
