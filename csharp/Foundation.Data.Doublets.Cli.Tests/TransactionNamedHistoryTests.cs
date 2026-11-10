using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionNamedHistoryTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void SelectedImagePreparationDoesNotCreateVersionsForUnchangedNames(bool explicitOwner)
    {
        var directory = Directory.CreateTempSubdirectory("image-noop-name-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var tx = new TransactionsDecorator(data, log, RowUndoPolicy.WriteAheadImage);
            using var vc = new VersionControlDecorator(tx, branches);
            var id = vc.CreateAndUpdate(11u, 12u); vc.SetName(id, "original");
            var before = vc.CurrentSequence; var records = tx.Journal.Count;
            using var pending = explicitOwner ? vc.BeginTransaction() : null;
            vc.SetName(id, "original"); vc.RemoveName(77);
            pending?.Commit();
            Assert.Equal(before, vc.CurrentSequence); Assert.Equal(records, tx.Journal.Count);
            vc.SetName(id, "following"); Assert.True(vc.CurrentSequence > before);
        }
        finally { Directory.Delete(directory, true); }
    }

    private sealed class NameMovingStore(string path) : NamedTypesDecorator<uint>(path)
    {
        internal string? NextName;
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, Platform.Delegates.WriteHandler<uint>? handler)
        {
            var result = base.Update(restriction, substitution, handler);
            var name = NextName; NextName = null;
            if (name is not null) SetName(substitution![0], name);
            return result;
        }
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public void UnobservedNameOwnerEvictionIsPreservedInRollbackAndVersionHistory(bool explicitOwner, bool image)
    {
        var directory = Directory.CreateTempSubdirectory("unobserved-name-").FullName;
        try
        {
            using var data = new NameMovingStore(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var tx = new TransactionsDecorator(data, log, image ? RowUndoPolicy.WriteAheadImage : RowUndoPolicy.Observed);
            using var vc = new VersionControlDecorator(tx, branches);
            var first = vc.CreateAndUpdate(11u, 12u); var second = vc.CreateAndUpdate(13u, 14u);
            vc.SetName(first, "original"); vc.SetName(second, "borrowed"); var before = vc.CurrentSequence;
            using var pending = explicitOwner ? vc.BeginTransaction() : null;
            data.NextName = "borrowed"; vc.Update(new[] { first }, new[] { first, 21u, 22u }, null);
            Assert.Equal("borrowed", vc.GetName(first)); Assert.Null(vc.GetName(second));
            if (explicitOwner) pending!.Rollback();
            else
            {
                var after = vc.CurrentSequence; vc.Checkout(before);
                Assert.Equal("original", vc.GetName(first)); Assert.Equal("borrowed", vc.GetName(second));
                vc.Checkout(after); Assert.Equal("borrowed", vc.GetName(first)); Assert.Null(vc.GetName(second));
                vc.Checkout(before);
            }
            Assert.Equal("original", vc.GetName(first)); Assert.Equal("borrowed", vc.GetName(second));
            Assert.Equal(11u, vc.GetSource(first)); vc.Update(new[] { first }, new[] { first, 31u, 32u }, null);
        }
        finally { Directory.Delete(directory, true); }
    }

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
    [InlineData("create", false)]
    [InlineData("update", false)]
    [InlineData("delete", false)]
    [InlineData("create", true)]
    [InlineData("update", true)]
    [InlineData("delete", true)]
    public void NameWritesInsideRowCallbacksSurviveVersionRoundTrips(string operation, bool image)
    {
        var directory = Directory.CreateTempSubdirectory("callback-name-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var tx = new TransactionsDecorator(data, log, image ? RowUndoPolicy.WriteAheadImage : RowUndoPolicy.Observed);
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
