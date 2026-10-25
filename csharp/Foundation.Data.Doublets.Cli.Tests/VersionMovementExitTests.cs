using Platform.Data;
using Platform.Data.Doublets;
using Fault = Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests.Fault;
using Metadata = Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests.Metadata;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMovementExitTests
{
    internal const string Topic = "topic|雪\0🚀";
    private static string ExitTopic(string point) => point.StartsWith("unpaired-high-", StringComparison.Ordinal)
        ? Topic + (char)0xD800 : point.StartsWith("unpaired-low-", StringComparison.Ordinal) ? Topic + (char)0xDFFF : Topic;
    public static TheoryData<string, bool> Cases
    {
        get
        {
            var cases = new TheoryData<string, bool>();
            foreach (var point in new[] { "intent-flush-after", "current-before", "current-after", "applied-after",
                "publication-flush-after", "intent-remove-after", "intent-delete-after", "cleanup-flush-after" })
                foreach (var branchSwitch in new[] { false, true }) cases.Add(point, branchSwitch);
            cases.Add("async-current-before", false);
            cases.Add("async-current-before", true);
            return cases;
        }
    }

    internal static int Child(string directory, string point, bool branchSwitch)
    {
        var topic = ExitTopic(point);
        if (point.StartsWith("unpaired-high-", StringComparison.Ordinal)) point = point["unpaired-high-".Length..];
        if (point.StartsWith("unpaired-low-", StringComparison.Ordinal)) point = point["unpaired-low-".Length..];
        using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
        using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
        var fault = new Fault();
        using var branches = new Metadata(Path.Combine(directory, "branches.links"), fault);
        using var checkpointFailed = new ManualResetEventSlim();
        var failCheckpoint = 0;
        void FlushData()
        {
            if (Interlocked.Exchange(ref failCheckpoint, 0) != 0)
            {
                checkpointFailed.Set();
                throw new IOException("Injected asynchronous checkpoint failure.");
            }
            data.Flush();
        }
        var asynchronous = point.StartsWith("async-", StringComparison.Ordinal);
        if (asynchronous) point = point[6..];
        using var tx = new TransactionsDecorator(data, log, flushData: FlushData, flushLog: log.Flush);
        using var vc = new VersionControlDecorator(tx, branches);
        var id = vc.CreateAndUpdate(11u, 12u);
        var original = vc.CurrentSequence;
        vc.Branch(topic);
        if (asynchronous) { tx.CommitMode = CommitMode.Async; failCheckpoint = 1; }
        vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
        if (asynchronous) Assert.True(checkpointFailed.Wait(TimeSpan.FromSeconds(3)));
        var attempted = vc.CurrentSequence;
        data.Flush(); log.Flush(); branches.Flush();
        File.WriteAllText(Path.Combine(directory, "manifest"), $"{id},{original},{attempted}");
        fault.Flushes = 0;
        fault.OnCheck = actual => { if (actual == point) Environment.Exit(0); };
        if (branchSwitch) vc.SwitchBranch(topic);
        else vc.Checkout(original);
        throw new InvalidOperationException("Movement did not reach its requested exit point.");
    }

    [Theory(Timeout = 30000)]
    [MemberData(nameof(Cases))]
    public async Task ExitAtEveryPersistedPhaseRecoversExactPosition(string point, bool branchSwitch)
    {
        var directory = Directory.CreateTempSubdirectory("version-movement-exit-").FullName;
        try
        {
            var (id, original, attempted) = await VersionProcessExitTests.ExitChild(directory, point, branchSwitch, "movement-exit");
            var selected = branchSwitch ? ExitTopic(point) : "main";
            long following = 0;
            for (var reopen = 0; reopen < 2; ++reopen)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal(selected, vc.CurrentBranch);
                Assert.Equal(reopen == 0 ? original : following, vc.CurrentSequence);
                Assert.Equal(reopen == 0 ? 11u : 31u, vc.GetSource(id));
                Assert.Equal(reopen == 0 ? attempted : following, tx.LastLoggedSequence);
                if (reopen == 0)
                {
                    vc.SwitchBranch("main");
                    vc.Checkout(attempted);
                    Assert.Equal(21u, vc.GetSource(id));
                    if (branchSwitch) vc.SwitchBranch(selected);
                    else vc.Checkout(original);
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    following = vc.CurrentSequence;
                }
                data.Flush(); log.Flush(); branches.Flush();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
