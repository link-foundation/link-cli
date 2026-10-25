using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionNamedHistoryTests
{
    private sealed class CaptureFaultStore(string path) : NamedTypesDecorator<uint>(path)
    {
        public bool FailCapture;
        public override IReadOnlyDictionary<uint, string> CaptureNames()
        {
            if (FailCapture) { FailCapture = false; throw new IOException("injected name capture"); }
            return base.CaptureNames();
        }
    }

    [Theory]
    [InlineData("rename")]
    [InlineData("remove")]
    [InlineData("move")]
    [InlineData("delete")]
    [InlineData("cascade")]
    public void RollbackRestoresNamesAndTheirUniqueOwners(string kind)
    {
        var directory = Directory.CreateTempSubdirectory("named-history-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var tx = new TransactionsDecorator(data, log);
            var first = tx.CreateAndUpdate(11u, 12u);
            var second = tx.CreateAndUpdate(first, 22u);
            tx.SetName(first, "first 雪|\0\n"); tx.SetName(second, "second");
            using var pending = tx.BeginTransaction();
            switch (kind)
            {
                case "rename": tx.SetName(first, "changed"); break;
                case "remove": tx.RemoveName(first); break;
                case "move": tx.SetName(second, "first 雪|\0\n"); break;
                case "delete": tx.Delete(new[] { second }, null); break;
                case "cascade": tx.Delete(new[] { first }, null); break;
            }
            pending.Rollback();
            Assert.True(tx.Exists(first)); Assert.True(tx.Exists(second));
            Assert.Equal("first 雪|\0\n", tx.GetName(first)); Assert.Equal("second", tx.GetName(second));
            Assert.Equal(first, tx.GetByName("first 雪|\0\n")); Assert.Equal(second, tx.GetByName("second"));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void NameOnlyVersionsAdvanceAndReplay()
    {
        var directory = Directory.CreateTempSubdirectory("named-version-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            var id = vc.CreateAndUpdate(11u, 12u);
            vc.SetName(id, "original"); var before = vc.CurrentSequence;
            vc.SetName(id, "changed 雪"); var after = vc.CurrentSequence;
            Assert.True(after > before, "A name-only write must advance version history.");
            vc.Checkout(before); Assert.Equal("original", vc.GetName(id));
            vc.Checkout(after); Assert.Equal("changed 雪", vc.GetName(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
    [Fact]
    public void AllocationRollbackPreservesANameAssignedBeforeItsRowExists()
    {
        var directory = Directory.CreateTempSubdirectory("external-name-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var tx = new TransactionsDecorator(data, log);
            tx.SetName(77, "external 雪");
            Assert.False(tx.Exists(77u));
            using var pending = tx.BeginTransaction();
            tx.EnsureCreated(77u);
            Assert.True(tx.Exists(77u));
            pending.Rollback();
            Assert.False(tx.Exists(77u));
            Assert.Equal("external 雪", tx.GetName(77));
            Assert.Equal(77u, tx.GetByName("external 雪"));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void EmptyNamesAndZeroAddressNamesKeepDistinctPresenceAndOwners()
    {
        var directory = Directory.CreateTempSubdirectory("empty-name-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var tx = new TransactionsDecorator(data, log);
            tx.SetName(0, ""); tx.SetName(77, "Type");
            using var pending = tx.BeginTransaction();
            tx.SetName(77, "");
            Assert.Null(tx.GetName(0));
            pending.Rollback();
            Assert.Equal("", tx.GetName(0));
            Assert.Equal("Type", tx.GetName(77));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void NonUniqueNamedLinksKeepTheirExistingSharedNameSemantics()
    {
        var directory = Directory.CreateTempSubdirectory("shared-name-").FullName;
        try
        {
            using var data = new NamedLinksDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var tx = new TransactionsDecorator(data, log);
            tx.SetName(11, "shared"); tx.SetName(22, "previous");
            using var pending = tx.BeginTransaction();
            tx.SetName(22, "shared");
            Assert.Equal("shared", tx.GetName(11)); Assert.Equal("shared", tx.GetName(22));
            pending.Rollback();
            Assert.Equal("shared", tx.GetName(11)); Assert.Equal("previous", tx.GetName(22));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData("create")]
    [InlineData("update")]
    [InlineData("delete")]
    public void NameWritesInsideRowCallbacksSurviveVersionRoundTrips(string operation)
    {
        var directory = Directory.CreateTempSubdirectory("callback-name-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            var id = vc.CreateAndUpdate(11u, 12u);
            vc.SetName(id, "original");
            var before = vc.CurrentSequence;
            uint created = 0;
            uint Rename(IList<uint>? previous, IList<uint>? next)
            {
                created = (next ?? previous)![0];
                vc.SetName(created, "callback 雪");
                return vc.Constants.Continue;
            }
            if (operation == "create") vc.Create(null, Rename);
            else if (operation == "update") vc.Update(new[] { id }, new[] { id, 21u, 22u }, Rename);
            else vc.Delete(new[] { id }, Rename);
            var after = vc.CurrentSequence;
            Assert.Equal("callback 雪", vc.GetName(created));
            vc.Checkout(before);
            Assert.Equal("original", vc.GetName(id));
            if (operation == "create") Assert.Null(vc.GetName(created));
            vc.Checkout(after);
            Assert.Equal("callback 雪", vc.GetName(created));
            Assert.Equal(operation != "delete", vc.Exists(created));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void PostEffectNameCaptureFailureRetainsUndoAndCleanupOwnership(bool explicitOwner)
    {
        var directory = Directory.CreateTempSubdirectory("capture-name-").FullName;
        try
        {
            using var data = new CaptureFaultStore(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var tx = new TransactionsDecorator(data, log);
            var id = tx.CreateAndUpdate(11u, 12u);
            tx.SetName(id, "original");
            using var pending = explicitOwner ? tx.BeginTransaction() : null;
            Assert.Contains("name capture", Assert.Throws<IOException>(() =>
                tx.Update(new[] { id }, new[] { id, 21u, 22u }, (_, _) =>
                { data.FailCapture = true; return tx.Constants.Continue; })).Message);
            if (explicitOwner) Assert.Throws<InvalidOperationException>(() => tx.SetName(id, "blocked"));
            tx.Rollback(); tx.Rollback();
            Assert.Equal(11u, tx.GetSource(id)); Assert.Equal(12u, tx.GetTarget(id));
            Assert.Equal("original", tx.GetName(id));
            tx.Update(new[] { id }, new[] { id, 31u, 32u }, null);
            Assert.Equal(31u, tx.GetSource(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

}
