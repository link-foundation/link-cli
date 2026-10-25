using System.Diagnostics;
using System.Runtime.InteropServices;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class JournalLiveTailTests
{
    [StructLayout(LayoutKind.Sequential)]
    private struct ResourceLimit { internal nuint Current, Maximum; }
    [DllImport("libc", EntryPoint = "getrlimit", SetLastError = true)]
    private static extern int GetLimit(int resource, out ResourceLimit limit);
    [DllImport("libc", EntryPoint = "setrlimit", SetLastError = true)]
    private static extern int SetLimit(int resource, ref ResourceLimit limit);

    [Theory(Timeout = 30000)]
    [InlineData("read", false)]
    [InlineData("read", true)]
    [InlineData("append", false)]
    [InlineData("append", true)]
    [InlineData("flush", false)]
    [InlineData("flush", true)]
    public async Task QuotaFailureRepairsTheLiveWriter(string mode, bool unicode)
    {
        if (!OperatingSystem.IsLinux()) return;
        var directory = Directory.CreateTempSubdirectory("journal-quota-").FullName;
        try
        {
            var info = new ProcessStartInfo("bash") { RedirectStandardOutput = true, RedirectStandardError = true };
            // The runtime double-mapping memfd also consumes the file-size quota.
            // Disable it only in this isolated child, so the limit tests journal I/O.
            info.Environment["DOTNET_EnableWriteXorExecute"] = "0";
            foreach (var argument in new[] { "-c", "trap '' XFSZ\nulimit -S -f 8\nexec \"$@\"", "quota-child", "dotnet",
                typeof(JournalLiveTailTests).Assembly.Location, "journal-quota", directory, mode, unicode.ToString() })
                info.ArgumentList.Add(argument);
            using var process = Process.Start(info)!;
            var stdout = process.StandardOutput.ReadToEndAsync();
            var stderr = process.StandardError.ReadToEndAsync();
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            try { await process.WaitForExitAsync(deadline.Token); }
            catch (OperationCanceledException)
            {
                process.Kill(entireProcessTree: true); await process.WaitForExitAsync();
                throw new TimeoutException("Finite journal quota child did not stop.");
            }
            Assert.True(process.ExitCode == 0, $"Child exited {process.ExitCode}:\n{await stdout}\n{await stderr}");
            using var reopened = new FileCommitLog(Path.Combine(directory, "journal"));
            Assert.Equal(new[] { "committed", "following" }, reopened.Read());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    internal static int QuotaChild(string directory, string mode, bool unicode)
    {
        const int fileSizeLimit = 1; // Linux RLIMIT_FSIZE.
        Assert.Equal(0, GetLimit(fileSizeLimit, out var limit));
        Assert.Equal((nuint)8192, limit.Current);
        try
        {
            var path = Path.Combine(directory, "journal");
            using var log = new FileCommitLog(path);
            log.Append("committed"); log.Flush();
            var candidate = unicode ? string.Concat(Enumerable.Repeat("雪", 6667)) : new string('x', 20000);
            var error = Record.Exception(() => log.Append(candidate));
            Assert.True(error is IOException or ArgumentOutOfRangeException, $"Expected quota error, got {error}");
            Console.Error.WriteLine($"Expected actual quota failure: {error}");
            var length = new FileInfo(path).Length;
            Assert.InRange(length, 11, 8192);
            if (mode == "read") Assert.Equal(new[] { "committed" }, log.Read());
            if (mode == "flush") { log.Flush(); Assert.Equal(new[] { "committed" }, log.Read()); }
            log.Append("following"); log.Flush();
            Assert.Equal(new[] { "committed", "following" }, log.Read());
            return 0;
        }
        finally
        {
            // Keep all journal operations bounded, then allow the coverage
            // collector to write its larger hits file during managed exit.
            limit.Current = limit.Maximum;
            Assert.Equal(0, SetLimit(fileSizeLimit, ref limit));
        }
    }
}
