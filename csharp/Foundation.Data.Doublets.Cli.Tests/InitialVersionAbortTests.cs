using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class InitialVersionAbortTests
{
    [Fact]
    public void ConstructionRejectsAnOpenBorrowedTransactionWithoutPublishingMetadata()
    {
        var directory = Directory.CreateTempSubdirectory("initial-version-open-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            using var tx = new TransactionsDecorator(data, log);
            using var pending = tx.BeginTransaction();
            var created = tx.CreateAndUpdate(21u, 22u);
            Assert.Throws<InvalidOperationException>(() => new VersionControlDecorator(tx, branches));
            var any = branches.Constants.Any;
            Assert.Empty(branches.All(new[] { any, any, any }));
            Assert.False(pending.IsCommitted);
            Assert.False(pending.IsRolledBack);
            Assert.Equal(21u, tx.GetSource(created));
            pending.Rollback();
            Assert.Equal(0u, tx.Count());
            using var vc = new VersionControlDecorator(tx, branches);
            Assert.Equal(0, vc.CurrentSequence);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void ExistingSidecarCannotAttributeAnAbortedRecord(bool pending)
    {
        var directory = Directory.CreateTempSubdirectory("invalid-version-abort-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            long sequence;
            uint id;
            using (var tx = new TransactionsDecorator(data, log))
            {
                id = tx.CreateAndUpdate(11u, 12u);
                var transaction = tx.BeginTransaction();
                tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                sequence = tx.LastLoggedSequence;
                if (!pending) transaction.Rollback();
                data.Flush(); log.Flush();
            }
            // Model the exact legacy initial-attribution error, including its
            // existing main branch, without asking the fixed constructor to do it.
            foreach (var marker in new[]
            {
                $"__vc:branch:main:parent=:fork=0:head={sequence}",
                $"__vc:trans:{sequence}:branch=main", "__vc:current=main",
                $"__vc:applied={sequence}"
            }) branches.SetName(branches.CreateAndUpdate(0u, 0u), marker);
            branches.Flush();
            using var recovered = new TransactionsDecorator(data, log);
            var error = Assert.Throws<InvalidDataException>(() => new VersionControlDecorator(recovered, branches));
            Assert.Contains(sequence.ToString(System.Globalization.CultureInfo.InvariantCulture), error.Message);
            Assert.Equal(11u, recovered.GetSource(id));
            var any = branches.Constants.Any;
            Assert.Equal(4, branches.All(new[] { any, any, any }).Count());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    public static TheoryData<string, bool> Cases => new()
    {
        { "all", false }, { "tail", false }, { "middle", false },
        { "all", true }, { "tail", true }, { "middle", true }
    };

    [Theory]
    [MemberData(nameof(Cases))]
    public void InitialHistoryExcludesAbortedAndRecoveredPendingRecords(string history, bool pending)
    {
        var directory = Directory.CreateTempSubdirectory("initial-version-abort-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        uint id = 0;
        long expectedHead = 0;
        long[] aborted;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log))
            {
                if (history != "all")
                {
                    id = tx.CreateAndUpdate(11u, 12u);
                    expectedHead = tx.LastLoggedSequence;
                }
                // Leave the handle undisposed for the pending-recovery case.
                var transaction = tx.BeginTransaction();
                if (history == "all") id = tx.CreateAndUpdate(21u, 22u);
                else tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                aborted = transaction.Transitions.Select(change => change.Sequence).ToArray();
                Assert.NotEmpty(aborted);
                if (!pending) transaction.Rollback();
                data.Flush(); log.Flush();
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(dataPath);
                using var log = new NamedTypesDecorator<uint>(logPath);
                using var branches = new NamedTypesDecorator<uint>(branchPath);
                using var tx = new TransactionsDecorator(data, log);
                if (history == "middle" && reopen == 0)
                {
                    tx.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    expectedHead = tx.LastLoggedSequence;
                }
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal(expectedHead, vc.CurrentSequence);
                Assert.Equal(expectedHead, vc.ListBranches().Single().Head);
                foreach (var sequence in aborted)
                {
                    Assert.Throws<InvalidOperationException>(() => vc.Checkout(sequence));
                    Assert.Throws<InvalidOperationException>(() => vc.Branch("aborted", sequence));
                }
                var any = branches.Constants.Any;
                var markers = branches.All(new[] { any, any, any }).Select(row => branches.GetName(Assert.IsAssignableFrom<IList<uint>>(row)[0])).ToArray();
                foreach (var sequence in aborted) Assert.DoesNotContain($"__vc:trans:{sequence}:branch=main", markers);
                Assert.Equal(history == "all" ? 0u : 1u, vc.Count());
                vc.Checkout(0);
                Assert.Equal(0u, vc.Count());
                vc.Checkout(expectedHead);
                Assert.Equal(history == "all" ? 0u : 1u, vc.Count());
                if (history != "all")
                {
                    Assert.Equal(history == "middle" ? 31u : 11u, vc.GetSource(id));
                    Assert.Equal(history == "middle" ? 32u : 12u, vc.GetTarget(id));
                }
                if (reopen == 0) vc.Tag("committed");
                Assert.True(vc.TryGetTag("committed", out var tag));
                Assert.Equal(expectedHead, tag);
                data.Flush(); log.Flush(); branches.Flush();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
