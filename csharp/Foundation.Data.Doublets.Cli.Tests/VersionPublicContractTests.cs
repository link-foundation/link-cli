using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionPublicContractTests
{
    [Fact]
    public async Task CancelledAsyncOperationsPreserveAdmissionAndCommittedHistory()
    {
        var directory = Directory.CreateTempSubdirectory("version-async-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches, trace: true);
            using var cancelled = new CancellationTokenSource(); cancelled.Cancel();
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() => vc.BeginTransactionAsync(cancelled.Token));
            var started = DateTimeOffset.UtcNow;
            using var pending = await vc.BeginTransactionAsync();
            Assert.NotEqual(Guid.Empty, pending.Id);
            Assert.InRange(pending.StartedAt, started, DateTimeOffset.UtcNow);
            Assert.Empty(pending.Transitions);
            var link = vc.CreateAndUpdate(11u, 12u);
            Assert.Equal(2, pending.Transitions.Count);
            Assert.All(pending.Transitions, transition => Assert.Equal(pending.Id, transition.TransactionId));
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() => pending.CommitAsync(cancelled.Token));
            Assert.False(pending.IsCommitted); Assert.False(pending.IsRolledBack);
            Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));
            await pending.CommitAsync();
            var head = vc.CurrentSequence;
            Assert.Equal(2, head); Assert.True(pending.IsCommitted);
            pending.Rollback(); pending.Dispose();
            Assert.Equal(head, vc.CurrentSequence); Assert.True(vc.Exists(link));
            vc.SetName(link, "kept");
            Assert.Equal("kept", vc.GetName(link)); Assert.Equal(link, vc.GetByName("kept"));
            vc.RemoveName(link);
            Assert.Null(vc.GetName(link)); Assert.Equal(0u, vc.GetByName("kept"));
            using (var following = await vc.BeginTransactionAsync())
            { vc.CreateAndUpdate(21u, 22u); }
            Assert.Equal(head, vc.CurrentSequence); Assert.Equal(1u, vc.Count());
            vc.Branch("following");
            vc.Tag("kept"); Assert.Equal(head, vc.ListTags()["kept"]);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void RejectedMetadataChangesPreserveDataAndAllowFollowingOperations()
    {
        var directory = Directory.CreateTempSubdirectory("version-invalid-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            var link = vc.CreateAndUpdate(11u, 12u);
            var head = vc.CurrentSequence;
            foreach (var name in new[] { null, "", " \t" })
            {
                Assert.Throws<ArgumentException>(() => vc.Branch(name!));
                Assert.Throws<ArgumentException>(() => vc.Tag(name!));
            }
            Assert.Throws<ArgumentOutOfRangeException>(() => vc.Branch("negative", -1));
            Assert.Throws<InvalidOperationException>(() => vc.Branch("missing", head + 1));
            Assert.Throws<InvalidOperationException>(() => vc.SwitchBranch("missing"));
            Assert.Throws<ArgumentOutOfRangeException>(() => vc.Checkout(-1));
            Assert.Throws<ArgumentOutOfRangeException>(() => vc.Tag("negative", -1));
            Assert.Equal(head, vc.CurrentSequence);
            Assert.Single(vc.ListBranches()); Assert.Empty(vc.ListTags());
            Assert.Equal(1u, vc.Count()); Assert.True(vc.Exists(link));
            Assert.Equal(11u, vc.GetSource(link)); Assert.Equal(12u, vc.GetTarget(link));
            vc.Branch("following", head); vc.SwitchBranch("following");
            vc.CreateAndUpdate(21u, 22u); vc.Tag("following");
            Assert.Equal(2u, vc.Count()); Assert.True(vc.CurrentSequence > head);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
