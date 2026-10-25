using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionRowUndoFailureTests
{
    private sealed record Image(Link<uint>[] Rows, Dictionary<uint, string> Names, string[] Journal)
    {
        internal static Image Empty => new([], [], []);
    }
    private sealed class Control
    {
        internal string? Point;
        internal int Skip;
        internal Image Image = Image.Empty;
        internal Image? Failed;
        internal void Check(string point)
        {
            if (Point != point) return;
            if (Skip > 0) { Skip--; return; }
            Point = null; Failed = Image; throw new IOException("injected " + point);
        }
    }
    private sealed class Store(string path, Control control) : NamedTypesDecorator<uint>(path)
    {
        internal bool Unobserved;
        internal bool HiddenOnName;
        internal bool FailAfterUnobserved;
        private bool _captureFailure;
        private void HiddenEffects()
        {
            Delete(new[] { 2u }, null);
            var created = this.CreateAndUpdate(51u, 52u); SetName(created, "hidden"); SetName(0u, "absent");
            _captureFailure = FailAfterUnobserved; FailAfterUnobserved = false;
        }
        public override IReadOnlyDictionary<uint, string> CaptureNames()
        {
            if (_captureFailure)
            {
                _captureFailure = false; Flush(); Capture(); control.Failed = control.Image;
                throw new IOException("injected resulting capture");
            }
            return base.CaptureNames();
        }
        internal void Capture() => control.Image = control.Image with
        {
            Rows = this.All(new Link<uint>(Constants.Any, Constants.Any, Constants.Any))
                .Select(row => new Link<uint>(row)).OrderBy(row => row.Index).ToArray(),
            Names = CaptureNames().ToDictionary(),
        };
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            control.Check("row-before");
            var result = base.Update(restriction, substitution, (before, after) =>
            {
                // Independently persist before forwarding any observation.
                Flush(); Capture(); control.Check("row-after");
                return handler is null ? Constants.Continue : handler(before, after);
            });
            if (Unobserved)
            {
                Unobserved = false; HiddenEffects();
            }
            return result;
        }
        public override uint SetName(uint index, string name)
        {
            control.Check("name-before"); var result = base.SetName(index, name);
            if (HiddenOnName) { HiddenOnName = false; HiddenEffects(); }
            Capture(); control.Check("name-after"); return result;
        }
    }
    private sealed class Log(string path, Control control) : NamedTypesDecorator<uint>(path)
    {
        internal string[] Entries() => this.All(new Link<uint>(Constants.Any, Constants.Any, Constants.Any))
            .Select(row => GetName(row![0])).OfType<string>().ToArray();
        public override uint SetName(uint index, string name)
        {
            control.Check("append-before"); var result = base.SetName(index, name);
            if (control.Point == "append-after" && control.Skip == 0)
                control.Image = control.Image with { Journal = Entries() };
            control.Check("append-after"); return result;
        }
    }
    private sealed class Harness : IDisposable
    {
        private readonly string _directory = Directory.CreateTempSubdirectory("row-undo-fault-").FullName;
        internal Control Control { get; } = new();
        internal Store Data { get; }
        internal Log Log { get; }
        internal TransactionsDecorator Tx { get; }
        internal Harness(Image image, bool select = false)
        {
            Data = new(Path.Combine(_directory, "data"), Control); Log = new(Path.Combine(_directory, "log"), Control);
            foreach (var row in image.Rows) Data.EnsureCreated(row.Index);
            foreach (var row in image.Rows) Data.Update(new[] { row.Index }, new[] { row.Index, row.Source, row.Target }, null);
            foreach (var pair in image.Names) Data.SetName(pair.Key, pair.Value);
            foreach (var entry in image.Journal) Log.SetName(Log.CreateAndUpdate(0u, 0u), entry);
            Control.Image = image;
            Tx = new(Data, Log, select ? RowUndoPolicy.WriteAheadImage : RowUndoPolicy.Observed, flushData: () =>
            {
                Control.Check("data-before"); Data.Flush(); Data.Capture(); Control.Check("data-after");
            }, flushLog: () =>
            {
                Control.Check("log-before"); Log.Flush();
                Control.Image = Control.Image with { Journal = Log.Entries() }; Control.Check("log-after");
            });
        }
        public void Dispose()
        {
            Tx.Dispose(); Log.Dispose(); Data.Dispose(); Directory.Delete(_directory, true);
        }
    }
    private static uint Seed(TransactionsDecorator tx)
    {
        var first = tx.CreateAndUpdate(11u, 12u); tx.SetName(first, "original 雪");
        var dependent = tx.CreateAndUpdate(first, first); tx.SetName(dependent, "dependent");
        tx.SetName(0, "zero"); tx.SetName(77, "absent"); return first;
    }
    private static void EqualImage(Image expected, Harness actual)
    {
        actual.Data.Capture();
        Assert.Equal(expected.Rows, actual.Control.Image.Rows);
        Assert.Equal(expected.Names.OrderBy(pair => pair.Key), actual.Control.Image.Names.OrderBy(pair => pair.Key));
    }
    public static TheoryData<string, string, int, bool, bool> Cases
    {
        get
        {
            var data = new TheoryData<string, string, int, bool, bool>();
            foreach (var (label, point, skip, commit) in new[]
            {
                ("prepare append before", "append-before", 0, false), ("prepare append after", "append-after", 0, false),
                ("prepare flush before", "log-before", 0, false), ("prepare flush after", "log-after", 0, false),
                ("row before", "row-before", 0, false), ("row after", "row-after", 0, false),
                ("row append before", "append-before", 1, false), ("row append after", "append-after", 1, false),
                ("commit append before", "append-before", 2, true), ("commit append after", "append-after", 2, true),
                ("commit flush before", "log-before", 1, true), ("commit flush after", "log-after", 1, true),
                ("checkpoint before", "data-before", 0, true), ("checkpoint after", "data-after", 0, true),
                ("checkpoint log before", "log-before", 2, true), ("checkpoint log after", "log-after", 2, true),
            }) foreach (var explicitOwner in new[] { false, true }) data.Add(label, point, skip, commit, explicitOwner);
            return data;
        }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void SelectedImageCompletesUnobservedEffectsForHistoryAndCommittedRedo(bool explicitOwner)
    {
        using var live = new Harness(Image.Empty, true); var tx = live.Tx;
        var first = Seed(tx); live.Data.Capture(); var before = live.Control.Image; var start = tx.Journal.Count;
        using var pending = explicitOwner ? tx.BeginTransaction() : null;
        live.Data.Unobserved = true; tx.Update(new[] { first }, new[] { first, 21u, 22u }, null); pending?.Commit();
        live.Data.Capture(); var after = live.Control.Image; var records = tx.Journal.Skip(start).ToArray();
        foreach (var record in records.Reverse()) tx.RevertRecord(record); EqualImage(before, live);
        foreach (var record in records) tx.ApplyRecord(record); EqualImage(after, live);
        var firstSequence = records[0].Sequence;
        var redo = after with
        {
            Rows = before.Rows,
            Names = before.Names,
            Journal = after.Journal.Where(entry =>
            !entry.StartsWith("__transactions:applied:", StringComparison.Ordinal)
            || long.Parse(entry["__transactions:applied:".Length..], System.Globalization.CultureInfo.InvariantCulture) < firstSequence).ToArray()
        };
        for (var reopen = 0; reopen < 2; reopen++)
        {
            using var restored = new Harness(redo); EqualImage(after, restored); redo = restored.Control.Image;
        }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void VersionPublicationIncludesUnobservedEffectsBeforeItsIntent(bool explicitOwner)
    {
        using var live = new Harness(Image.Empty, true);
        var first = Seed(live.Tx); live.Data.Capture(); var before = live.Control.Image;
        var directory = Directory.CreateTempSubdirectory("image-version-").FullName;
        try
        {
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var vc = new VersionControlDecorator(live.Tx, branches);
            var old = vc.CurrentSequence;
            using var pending = explicitOwner ? vc.BeginTransaction() : null;
            live.Data.Unobserved = true; vc.Update(new[] { first }, new[] { first, 21u, 22u }, null); pending?.Commit();
            live.Data.Capture(); var after = live.Control.Image; var next = vc.CurrentSequence;
            Assert.Equal(live.Tx.LastLoggedSequence, next);
            for (var replay = 0; replay < 2; replay++)
            {
                vc.Checkout(old); EqualImage(before, live);
                vc.Checkout(next); EqualImage(after, live);
            }
            vc.SetName(first, "following");
        }
        finally { Directory.Delete(directory, true); }
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public void NameWriterUnreportedEffectsHaveCompleteHistory(bool explicitOwner, bool sameName)
    {
        using var live = new Harness(Image.Empty, true); var tx = live.Tx;
        var first = Seed(tx); live.Data.Capture(); var before = live.Control.Image; var start = tx.Journal.Count;
        using var pending = explicitOwner ? tx.BeginTransaction() : null;
        live.Data.HiddenOnName = true; tx.SetName(first, sameName ? "original 雪" : "changed"); pending?.Commit();
        live.Data.Capture(); var after = live.Control.Image; var records = tx.Journal.Skip(start).ToArray();
        foreach (var record in records.Reverse()) tx.RevertRecord(record); EqualImage(before, live);
        foreach (var record in records) tx.ApplyRecord(record); EqualImage(after, live);
        using var reopened = new Harness(after); EqualImage(after, reopened);
    }

    [Theory]
    [InlineData("capture", false)]
    [InlineData("capture", true)]
    [InlineData("append-before", false)]
    [InlineData("append-before", true)]
    [InlineData("append-after", false)]
    [InlineData("append-after", true)]
    public void FailedResultCaptureOrCorrectionAppendRetainsCompleteImageUndo(string point, bool explicitOwner)
    {
        using var live = new Harness(Image.Empty, true); var tx = live.Tx;
        var first = Seed(tx); live.Data.Capture(); var before = live.Control.Image;
        using var pending = explicitOwner ? tx.BeginTransaction() : null;
        live.Data.Unobserved = true;
        if (point == "capture") live.Data.FailAfterUnobserved = true;
        else { live.Control.Point = point; live.Control.Skip = 2; }
        Assert.Throws<IOException>(() => tx.Update(new[] { first }, new[] { first, 21u, 22u }, null));
        var image = Assert.IsType<Image>(live.Control.Failed);
        for (var reopen = 0; reopen < 2; reopen++)
        {
            using var restored = new Harness(image); EqualImage(before, restored); image = restored.Control.Image;
        }
        if (explicitOwner)
        {
            Assert.Throws<InvalidOperationException>(() => pending!.Commit());
            Assert.Throws<InvalidOperationException>(() => tx.SetName(first, "blocked"));
        }
        pending?.Rollback(); tx.Rollback(); tx.Rollback(); EqualImage(before, live);
        tx.CreateAndUpdate(31u, 32u);
    }

    [Theory]
    [MemberData(nameof(Cases))]
    public void PreparationRowAndCommitFailuresRecoverImagesAndRetainCleanup(string label, string point, int skip, bool attemptedCommit, bool explicitOwner)
    {
        using var live = new Harness(Image.Empty, true);
        var tx = live.Tx; var first = Seed(tx); live.Data.Capture(); var before = live.Control.Image;
        var oldCommits = before.Journal.Where(entry => entry.StartsWith("__transactions:commit:", StringComparison.Ordinal)).ToHashSet();
        using var pending = explicitOwner ? tx.BeginTransaction() : null;
        live.Control.Point = point; live.Control.Skip = skip;
        Assert.Throws<IOException>(() => { tx.Update(new[] { first }, new[] { first, 21u, 22u }, null); pending?.Commit(); });
        Assert.True(live.Control.Point is null, "fault must fire: " + label);
        var image = Assert.IsType<Image>(live.Control.Failed);
        var committed = image.Journal.Any(entry => entry.StartsWith("__transactions:commit:", StringComparison.Ordinal) && !oldCommits.Contains(entry));
        for (var reopen = 0; reopen < 2; reopen++)
        {
            using var restored = new Harness(image);
            Assert.Equal(committed ? 21u : 11u, restored.Tx.GetSource(first));
            Assert.Equal(committed ? 22u : 12u, restored.Tx.GetTarget(first));
            Assert.Equal("original 雪", restored.Tx.GetName(first));
            var following = restored.Tx.CreateAndUpdate(31u, 32u); restored.Tx.SetName(following, "following " + reopen);
            image = restored.Control.Image;
        }
        if (explicitOwner || attemptedCommit)
        {
            Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(41u, 42u));
            Assert.Throws<InvalidOperationException>(() => tx.SetName(first, "blocked"));
            Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
        }
        tx.Rollback(); tx.Rollback();
        if (attemptedCommit) { Assert.Equal(21u, tx.GetSource(first)); Assert.Equal(22u, tx.GetTarget(first)); }
        else EqualImage(before, live);
        tx.CreateAndUpdate(41u, 42u);
    }

    [Theory]
    [InlineData("row-before")]
    [InlineData("row-after")]
    [InlineData("name-before")]
    [InlineData("name-after")]
    [InlineData("data-before")]
    [InlineData("data-after")]
    [InlineData("append-before")]
    [InlineData("append-after")]
    [InlineData("log-before")]
    [InlineData("log-after")]
    public void FailedImageUndoRetainsOwnerThroughRepeatedCleanup(string point)
    {
        using var live = new Harness(Image.Empty, true); var tx = live.Tx;
        var first = Seed(tx); live.Data.Capture(); var before = live.Control.Image;
        using var pending = tx.BeginTransaction();
        tx.Update(new[] { first }, new[] { first, 21u, 22u }, null); tx.SetName(first, "changed");
        for (var retry = 0; retry < 2; retry++)
        {
            live.Control.Point = point;
            Assert.Throws<IOException>(tx.Rollback); Assert.Null(live.Control.Point);
            Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(41u, 42u));
            Assert.Throws<InvalidOperationException>(pending.Commit); Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
            using var restored = new Harness(Assert.IsType<Image>(live.Control.Failed)); EqualImage(before, restored);
        }
        tx.Rollback(); tx.Rollback(); EqualImage(before, live); tx.CreateAndUpdate(41u, 42u);
    }
}
