using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionPublicationContractTests
{
    [Theory]
    [InlineData(false, false, false)]
    [InlineData(false, false, true)]
    [InlineData(false, true, false)]
    [InlineData(false, true, true)]
    [InlineData(true, false, false)]
    [InlineData(true, false, true)]
    [InlineData(true, true, false)]
    [InlineData(true, true, true)]
    public void DurablePublicationRequiresEverySelectedStore(bool dataFlush, bool logFlush, bool branchesFlush)
    {
        var directory = Directory.CreateTempSubdirectory("version-publication-capability-").FullName;
        try
        {
            NamedTypesDecorator<uint> Store(string name, bool owned)
            {
                var path = Path.Combine(directory, name + ".links");
                return owned ? new NamedTypesDecorator<uint>(path)
                    : new NamedTypesDecorator<uint>(NamedTypesDecorator<uint>.MakeLinks(path),
                        NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(path));
            }
            using var data = Store("data", dataFlush);
            using var log = Store("log", logFlush);
            using var branches = Store("branches", branchesFlush);
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            Assert.Equal(dataFlush && logFlush && branchesFlush, vc.SupportsDurablePublication);
            var id = vc.CreateAndUpdate(11u, 12u);
            Assert.Equal(11u, vc.GetSource(id));
            Assert.Equal(2, vc.CurrentSequence);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void ExistingBinariesCanResolveTheOriginalConstructor()
    {
        Assert.NotNull(typeof(VersionControlDecorator).GetConstructor(new[]
        {
            typeof(TransactionsDecorator), typeof(INamedTypesLinks<uint>), typeof(bool)
        }));
    }

    [Fact]
    public void CallerFlushPersistsIntentBeforeDecisionAndPublicationBeforeCleanup()
    {
        var directory = Directory.CreateTempSubdirectory("version-publication-contract-").FullName;
        try
        {
            uint id;
            using (var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links")))
            using (var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links")))
            using (var tx = new TransactionsDecorator(data, log))
            {
                Guid decision = Guid.Empty;
                var observed = new List<(bool Intent, bool Committed, string Applied)>();
                void Flush()
                {
                    var any = branches.Constants.Any;
                    var names = branches.All(new Link<uint>(any, any, any))
                        .Where(row => row is not null).Select(row => branches.GetName(row![0])).ToArray();
                    observed.Add((names.Any(name => name?.StartsWith("__vc:pending:", StringComparison.Ordinal) == true),
                        tx.IsTransactionCommitted(decision), names.Single(name =>
                            name?.StartsWith("__vc:applied=", StringComparison.Ordinal) == true)!));
                    branches.Flush();
                }
                using var vc = new VersionControlDecorator(tx, branches, flushBranches: Flush);
                Assert.True(vc.SupportsDurablePublication);
                Assert.Empty(observed);
                using var pending = vc.BeginTransaction();
                decision = pending.Id;
                id = vc.CreateAndUpdate(11u, 12u);
                pending.Commit();
                Assert.Equal(new[]
                {
                    (true, false, "__vc:applied=0"),
                    (true, true, "__vc:applied=2"),
                    (false, true, "__vc:applied=2")
                }, observed);
            }
            using var reopenedData = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var reopenedLog = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var reopenedBranches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            using var reopenedTx = new TransactionsDecorator(reopenedData, reopenedLog);
            using var reopened = new VersionControlDecorator(reopenedTx, reopenedBranches);
            Assert.Equal(2, reopened.CurrentSequence);
            Assert.Equal(11u, reopened.GetSource(id));
            reopened.Checkout(0);
            Assert.False(reopened.Exists(id));
            reopened.Checkout(2);
            Assert.Equal(11u, reopened.GetSource(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
