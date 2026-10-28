using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;
using Fault = Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests.Fault;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMovementDataFailureTests
{
    private sealed class Data(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        internal bool Armed;
        internal int Updates;
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            var count = Armed ? ++Updates : 0;
            if (Armed) fault.Check($"row{count}-before");
            var result = base.Update(restriction, substitution, handler);
            if (Armed) fault.Check($"row{count}-after");
            return result;
        }
    }

    public static TheoryData<string> Cases
    {
        get
        {
            var cases = new TheoryData<string>();
            foreach (var phase in new[] { "row1", "row2", "data1", "data2", "log1", "log2" })
                foreach (var edge in new[] { "before", "after" }) cases.Add(phase + "-" + edge);
            return cases;
        }
    }

    [Theory(Timeout = 30000)]
    [MemberData(nameof(Cases))]
    public async Task PartialReplayAndDataFlushFailuresRetainWholeGraph(string point)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("movement-data-fault-").FullName;
        try
        {
            uint id;
            long following;
            using (var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links")))
            {
                var fault = new Fault();
                using var data = new Data(Path.Combine(directory, "data.links"), fault);
                var dataFlushes = 0;
                var logFlushes = 0;
                void FlushData()
                {
                    var count = data.Armed ? ++dataFlushes : 0;
                    fault.Check("data-before");
                    if (data.Armed) fault.Check($"data{count}-before");
                    data.Flush();
                    if (data.Armed) fault.Check($"data{count}-after");
                }
                void FlushLog()
                {
                    var count = data.Armed ? ++logFlushes : 0;
                    if (data.Armed) fault.Check($"log{count}-before");
                    log.Flush();
                    if (data.Armed) fault.Check($"log{count}-after");
                }
                using var tx = new TransactionsDecorator(data, log, flushData: FlushData, flushLog: FlushLog);
                using var vc = new VersionControlDecorator(tx, branches, trace: true);
                id = vc.CreateAndUpdate(11u, 12u);
                vc.Branch("topic");
                vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                vc.SwitchBranch("topic");
                vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                var attempted = tx.LastLoggedSequence;
                data.Armed = true; fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(() => vc.SwitchBranch("main")).Message);
                Assert.Null(fault.Point);
                Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(51u, 52u));
                Assert.Throws<InvalidOperationException>(tx.Shutdown);
                fault.Point = "data-before";
                Assert.Throws<IOException>(() => LinksFacadeDisposer.Dispose(vc));
                Assert.True(data.Exists(id));
                Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
                vc.Dispose();
                Assert.Null(fault.Point);
                Assert.Equal("main", vc.CurrentBranch);
                Assert.Equal(21u, vc.GetSource(id));
                Assert.Equal(attempted, tx.LastLoggedSequence);
                vc.Update(new[] { id }, new[] { id, 41u, 42u }, null);
                following = vc.CurrentSequence;
                Assert.Equal(attempted + 1, following);
                data.Flush(); log.Flush(); branches.Flush();
            }
            for (var reopen = 0; reopen < 2; ++reopen)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal("main", vc.CurrentBranch);
                Assert.Equal(following, vc.CurrentSequence);
                Assert.Equal(41u, vc.GetSource(id));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
