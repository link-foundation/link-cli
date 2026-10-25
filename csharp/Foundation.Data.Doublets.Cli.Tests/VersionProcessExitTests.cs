using System.Diagnostics;
using System.Globalization;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionProcessExitTests
{
    internal static async Task<(uint Id, long Original, long Attempted)> ExitChild(string directory, string mode, bool automatic, string probe = "version-exit")
    {
        var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
        info.ArgumentList.Add(typeof(VersionProcessExitTests).Assembly.Location);
        foreach (var argument in new[] { probe, directory, mode, automatic.ToString() }) info.ArgumentList.Add(argument);
        using var process = Process.Start(info)!;
        var stdout = process.StandardOutput.ReadToEndAsync();
        var stderr = process.StandardError.ReadToEndAsync();
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
        try { await process.WaitForExitAsync(deadline.Token); }
        catch (OperationCanceledException)
        {
            process.Kill(entireProcessTree: true);
            await process.WaitForExitAsync();
            throw new TimeoutException("Finite version-exit child failed to stop.");
        }
        Assert.True(process.ExitCode == 0, $"Child exited {process.ExitCode}:\n{await stdout}\n{await stderr}");
        var fields = File.ReadAllText(Path.Combine(directory, "manifest")).Split(',');
        return (uint.Parse(fields[0], CultureInfo.InvariantCulture),
            long.Parse(fields[1], CultureInfo.InvariantCulture), long.Parse(fields[2], CultureInfo.InvariantCulture));
    }

    [Theory(Timeout = 30000)]
    [InlineData("success", false)]
    [InlineData("success", true)]
    [InlineData("checkpoint", false)]
    [InlineData("checkpoint", true)]
    [InlineData("pending", false)]
    [InlineData("rollback", false)]
    public async Task AbruptExitRecoversExactBranchAttribution(string mode, bool automatic)
    {
        var directory = Directory.CreateTempSubdirectory("version-process-exit-").FullName;
        try
        {
            var (id, original, attempted) = await ExitChild(directory, mode, automatic);
            var committed = mode is "success" or "checkpoint";
            var expected = committed ? attempted : original;
            long following = 0;
            for (var reopen = 0; reopen < 2; ++reopen)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal(VersionExitProbe.Topic, vc.CurrentBranch);
                Assert.Equal(reopen == 0 ? expected : following, vc.CurrentSequence);
                Assert.Equal(reopen == 0 ? expected : following, vc.ListBranches().Single(b => b.Name == VersionExitProbe.Topic).Head);
                Assert.Equal(reopen == 1 ? 31u : committed ? 21u : 11u, vc.GetSource(id));
                if (reopen == 0)
                {
                    vc.Checkout(original);
                    Assert.Equal(11u, vc.GetSource(id));
                    if (committed)
                    {
                        vc.Checkout(attempted);
                        Assert.Equal(21u, vc.GetSource(id));
                    }
                    else Assert.Throws<InvalidOperationException>(() => vc.Checkout(attempted));
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    following = vc.CurrentSequence;
                }
                data.Flush(); log.Flush(); branches.Flush();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
