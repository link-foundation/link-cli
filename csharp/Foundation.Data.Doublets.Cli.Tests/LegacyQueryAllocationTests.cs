using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed class LegacyQueryAllocationTests
{
    private static void WithLinks(Action<NamedTypesDecorator<uint>> action)
    {
        var directory = Directory.CreateTempSubdirectory("recycling-query-").FullName;
        try
        {
            using var links = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            action(links);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private static Link<uint>[] Rows(INamedTypesLinks<uint> links) => links.All()
        .Select(raw => new Link<uint>(raw)).OrderBy(row => row.Index).ToArray();

    [Theory]
    [InlineData("() ((1: 1 1) (2 2))", 1u, 2u)]
    [InlineData("() ((foo: foo foo) (2 2))", 1u, 2u)]
    [InlineData("() ((100: 100 100) (99 99))", 99u, 100u)]
    public void ValidationUsesTheRealFollowingAllocation(string query, uint first, uint second)
    {
        WithLinks(links =>
        {
            AdvancedMixedQueryProcessor.ProcessQuery(links, query);
            Assert.Equal(new[] { new Link<uint>(first, first, first), new Link<uint>(second, second, second) }, Rows(links));
            if (query.Contains("foo", StringComparison.Ordinal)) Assert.Equal(1u, links.GetByName("foo"));
        });
    }

    [Theory]
    [InlineData("() ((100: 100 100) (1 1))")]
    [InlineData("() ((foo: (1 1) (1 1)) (3 3))")]
    public void RejectedPredictionsDoNotWriteRowsOrConsumeTheAllocator(string query)
    {
        WithLinks(links =>
        {
            Assert.ThrowsAny<Exception>(() => AdvancedMixedQueryProcessor.ProcessQuery(links, query));
            Assert.Empty(Rows(links));
            Assert.Equal(1u, ((INamedTypesLinks<uint>)links).Create());
        });
    }

    private static byte[] ReadFile(string path)
    {
        using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        using var bytes = new MemoryStream();
        file.CopyTo(bytes);
        return bytes.ToArray();
    }

    [Fact]
    public void RefusalPreservesBothFilesNamesAndFreeListOrder()
    {
        WithLinks(links =>
        {
            for (uint id = 1; id <= 4; id++) links.CreatePoint();
            links.SetName(1, "retained λ");
            links.Delete(2u);
            links.Delete(3u);
            links.Flush();
            var dataPath = Path.Combine(Path.GetDirectoryName(links.NamedLinksDatabaseFileName)!, "data.links");
            var beforeData = ReadFile(dataPath);
            var beforeNames = ReadFile(links.NamedLinksDatabaseFileName);
            Assert.Throws<InvalidOperationException>(() => AdvancedMixedQueryProcessor.ProcessQuery(links, "() ((100: 100 100) (2 2))"));
            Assert.Equal(beforeData, ReadFile(dataPath));
            Assert.Equal(beforeNames, ReadFile(links.NamedLinksDatabaseFileName));
            Assert.Equal("retained λ", links.GetName(1));
            Assert.Equal(3u, ((INamedTypesLinks<uint>)links).Create());
            Assert.Equal(2u, ((INamedTypesLinks<uint>)links).Create());
        });
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void PreviewOwnsMutationsAndRetainedInterfaceHasAFiniteLifetime(bool fail)
    {
        WithLinks(links =>
        {
            var first = links.CreatePoint();
            links.SetName(first, "before");
            INamedTypesLinks<uint>? retained = null;
            void Preview(INamedTypesLinks<uint> copy)
            {
                retained = copy;
                copy.SetName(first, "preview");
                Assert.Equal(2u, copy.Create());
                if (fail) throw new FormatException("injected preview failure");
            }
            if (fail) Assert.Throws<FormatException>(() => links.ReserveQueryAllocations(Preview));
            else Assert.True(links.ReserveQueryAllocations(Preview)!.SetEquals([1, 2]));
            Assert.Equal("before", links.GetName(first));
            Assert.Single(Rows(links));
            Assert.Equal(2u, ((INamedTypesLinks<uint>)links).Create());
            Assert.Throws<ObjectDisposedException>(() => retained!.Count(null));
            Assert.Throws<ObjectDisposedException>(() => retained!.Each(null, null));
            Assert.Throws<ObjectDisposedException>(() => retained!.Create(null, null));
            Assert.Throws<ObjectDisposedException>(() => retained!.Update(null, null, null));
            Assert.Throws<ObjectDisposedException>(() => retained!.Delete(null, null));
            Assert.Throws<ObjectDisposedException>(() => retained!.GetName(first));
            Assert.Throws<ObjectDisposedException>(() => retained!.GetByName("before"));
            Assert.Throws<ObjectDisposedException>(() => retained!.SetName(first, "late"));
            Assert.Throws<ObjectDisposedException>(() => retained!.RemoveName(first));
            Assert.Throws<ObjectDisposedException>(() => retained!.CaptureNames());
        });
    }

    [Fact]
    public void TransactionPreviewDoesNotLogAndActualWritesStillRollBack()
    {
        WithLinks(data => WithLinks(log =>
        {
            using var tx = new TransactionsDecorator(data, log);
            Assert.Throws<InvalidOperationException>(() => AdvancedMixedQueryProcessor.ProcessQuery(tx, "() ((100: 100 100) (1 1))"));
            Assert.Empty(tx.Log);
            using var owner = tx.BeginTransaction();
            AdvancedMixedQueryProcessor.ProcessQuery(tx, "() ((1: 1 1) (2 2))");
            Assert.Equal(2, Rows(tx).Length);
            Assert.Equal(4, tx.Log.Count);
            owner.Rollback();
            Assert.Empty(Rows(tx));
        }));
    }

    [Fact]
    public void VersionPreviewDoesNotPublishAndCheckoutRetainsActualWrites()
    {
        WithLinks(data => WithLinks(log => WithLinks(branches =>
        {
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            Assert.Throws<InvalidOperationException>(() => AdvancedMixedQueryProcessor.ProcessQuery(vc, "() ((100: 100 100) (1 1))"));
            Assert.Empty(tx.Log);
            Assert.Equal(0, vc.CurrentSequence);
            AdvancedMixedQueryProcessor.ProcessQuery(vc, "() ((1: 1 1) (2 2))");
            var head = vc.CurrentSequence;
            Assert.True(head > 0);
            var expected = Rows(vc);
            Assert.Equal(2, expected.Length);
            vc.Checkout(0);
            Assert.Empty(Rows(vc));
            vc.Checkout(head);
            Assert.Equal(expected, Rows(vc));
        })));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void PreviewWaitsForCommittedCheckpointBeforeCallingThePreview(bool version)
    {
        WithLinks(data => WithLinks(log => WithLinks(branches =>
        {
            var fail = false;
            using var tx = new TransactionsDecorator(data, log, flushData: () =>
            {
                data.Flush();
                if (fail) { fail = false; throw new IOException("injected checkpoint failure"); }
            });
            using var vc = new VersionControlDecorator(tx, branches);
            INamedTypesLinks<uint> store = version ? vc : tx;
            using var owner = version ? vc.BeginTransaction() : tx.BeginTransaction();
            store.CreatePoint();
            fail = true;
            Assert.Throws<IOException>(owner.Commit);
            Assert.True(owner.IsCommitted);
            var sequence = tx.LastLoggedSequence;
            var calls = 0;
            Assert.Throws<InvalidOperationException>(() => store.ReserveQueryAllocations(_ => calls++));
            Assert.Equal(0, calls);
            Assert.Equal(sequence, tx.LastLoggedSequence);
            owner.Commit();
            Assert.NotNull(store.ReserveQueryAllocations(_ => calls++));
            Assert.Equal(1, calls);
        })));
    }

    private sealed class CustomStore(string path) : NamedTypesDecorator<uint>(path);
    private sealed class CustomTransactions(INamedTypesLinks<uint> data, INamedTypesLinks<uint> log) : TransactionsDecorator(data, log);
    private sealed class CustomVersions(TransactionsDecorator tx, INamedTypesLinks<uint> branches) : VersionControlDecorator(tx, branches);

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void InheritedJournalAndHistoryDecoratorsKeepTheirCompatibilityFallback(bool version)
    {
        WithLinks(data => WithLinks(log => WithLinks(branches =>
        {
            using var tx = version ? new TransactionsDecorator(data, log) : new CustomTransactions(data, log);
            using var vc = new CustomVersions(tx, branches);
            INamedTypesLinks<uint> store = version ? vc : tx;
            var calls = 0;
            Assert.Null(store.ReserveQueryAllocations(_ => calls++));
            Assert.Equal(0, calls);
            Assert.Empty(tx.Log);
        })));
    }

    [Fact]
    public void BorrowedAndInheritedAllocatorsKeepTheirCompatibilityFallback()
    {
        WithLinks(data =>
        {
            var directory = Path.GetDirectoryName(data.NamedLinksDatabaseFileName)!;
            using var custom = new CustomStore(Path.Combine(directory, "custom.links"));
            using var borrowed = new NamedTypesDecorator<uint>(data, Path.Combine(directory, "borrowed.names.links"));
            var calls = 0;
            Assert.Null(custom.ReserveQueryAllocations(_ => calls++));
            Assert.Null(borrowed.ReserveQueryAllocations(_ => calls++));
            Assert.Equal(0, calls);
            Assert.Throws<ArgumentNullException>(() => data.ReserveQueryAllocations(null!));
            data.Dispose();
            Assert.Throws<ObjectDisposedException>(() => data.ReserveQueryAllocations(_ => calls++));
        });
    }

    [Fact]
    public void OwnedUlongAllocatorUsesTheSameExactPreview()
    {
        var directory = Directory.CreateTempSubdirectory("wide-recycling-query-").FullName;
        try
        {
            using var links = new NamedTypesDecorator<ulong>(Path.Combine(directory, "data.links"));
            AdvancedMixedQueryProcessor<ulong>.ProcessQuery(links, "() ((100: 100 100) (99 99))");
            Assert.Equal(new[] { 99UL, 100UL }, links.All().Select(row => new Link<ulong>(row).Index).Order());
            Assert.Equal(98UL, ((INamedTypesLinks<ulong>)links).Create());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
