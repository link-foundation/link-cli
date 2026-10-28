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
        await FiniteExitChild.Run(info, 0, $"{probe}/{mode}/{automatic}");
        var fields = File.ReadAllText(Path.Combine(directory, "manifest")).Split(',');
        return (uint.Parse(fields[0], CultureInfo.InvariantCulture),
            long.Parse(fields[1], CultureInfo.InvariantCulture), long.Parse(fields[2], CultureInfo.InvariantCulture));
    }

    [Theory(Timeout = 30000)]
    [InlineData("success", false, false)]
    [InlineData("success", true, false)]
    [InlineData("checkpoint", false, false)]
    [InlineData("checkpoint", true, false)]
    [InlineData("pending", false, false)]
    [InlineData("rollback", false, false)]
    [InlineData("success", false, true)]
    [InlineData("success", true, true)]
    [InlineData("checkpoint", false, true)]
    [InlineData("checkpoint", true, true)]
    [InlineData("pending", false, true)]
    [InlineData("rollback", false, true)]
    public async Task AbruptExitRecoversExactBranchAttribution(string mode, bool automatic, bool names)
    {
        await FiniteExitChild.WithDirectory("version-process-exit-", async directory =>
        {
            var (id, original, attempted) = await ExitChild(directory, mode, automatic, names ? "name-exit" : "version-exit");
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
                Assert.Equal(reopen == 1 ? 31u : committed && !names ? 21u : 11u, vc.GetSource(id));
                Assert.Equal(committed && names ? "changed 雪|\0\n" : "original", vc.GetName(id));
                if (reopen == 0)
                {
                    vc.Checkout(original);
                    Assert.Equal(11u, vc.GetSource(id));
                    Assert.Equal("original", vc.GetName(id));
                    if (committed)
                    {
                        vc.Checkout(attempted);
                        Assert.Equal(names ? 11u : 21u, vc.GetSource(id));
                        Assert.Equal(names ? "changed 雪|\0\n" : "original", vc.GetName(id));
                    }
                    else Assert.Throws<InvalidOperationException>(() => vc.Checkout(attempted));
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    following = vc.CurrentSequence;
                }
                data.Flush(); log.Flush(); branches.Flush();
            }
        });
    }
}
