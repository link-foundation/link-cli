using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionNameFailureTests
{
    private sealed record Image(Link<uint>[] Rows, Dictionary<uint, string> Names, string[] Journal)
    {
        public static Image Empty => new([], [], []);
    }
    private sealed class Control
    {
        public string? Point;
        public int Skip;
        public Image Image = Image.Empty;
        public Image? Failed;
        public void Check(string point)
        {
            if (Point != point) return;
            if (Skip > 0) { Skip--; return; }
            Point = null; Failed = Image;
            throw new IOException("injected " + point);
        }
    }
    private sealed class Store(string path, Control control) : NamedTypesDecorator<uint>(path)
    {
        public override uint SetName(uint link, string name)
        {
            control.Check("name-before");
            var result = base.SetName(link, name);
            control.Image = control.Image with { Names = CaptureNames().ToDictionary() };
            control.Check("name-after");
            return result;
        }
    }
    private sealed class Log(string path, Control control) : NamedTypesDecorator<uint>(path)
    {
        public string[] Entries() => this.All(new Link<uint>(Constants.Any, Constants.Any, Constants.Any))
            .Select(row => GetName(row![0])).OfType<string>().ToArray();
        public override uint SetName(uint link, string name)
        {
            control.Check("append-before");
            var result = base.SetName(link, name);
            if (control.Point == "append-after" && control.Skip == 0) control.Image = control.Image with { Journal = Entries() };
            control.Check("append-after");
            return result;
        }
    }
    private sealed class Harness : IDisposable
    {
        private readonly string _directory = Directory.CreateTempSubdirectory("name-fault-").FullName;
        public Control Control { get; } = new();
        public Store Data { get; }
        public Log Log { get; }
        public TransactionsDecorator Tx { get; }
        public Harness(Image image)
        {
            Data = new(Path.Combine(_directory, "data"), Control);
            Log = new(Path.Combine(_directory, "log"), Control);
            foreach (var row in image.Rows) Data.EnsureCreated(row.Index);
            foreach (var row in image.Rows) Data.Update(new[] { row.Index }, new[] { row.Index, row.Source, row.Target }, null);
            foreach (var name in image.Names) Data.SetName(name.Key, name.Value);
            foreach (var entry in image.Journal) Log.SetName(Log.CreateAndUpdate(0u, 0u), entry);
            Control.Image = image;
            Tx = new(Data, Log, flushData: () =>
            {
                Control.Check("data-before");
                Data.Flush();
                Control.Image = Control.Image with
                {
                    Rows = Data.All(new Link<uint>(Data.Constants.Any, Data.Constants.Any, Data.Constants.Any)).Select(row => new Link<uint>(row)).ToArray(),
                    Names = Data.CaptureNames().ToDictionary(),
                };
                Control.Check("data-after");
            }, flushLog: () =>
            {
                Control.Check("log-before"); Log.Flush();
                Control.Image = Control.Image with { Journal = Log.Entries() };
                Control.Check("log-after");
            });
        }
        public void Dispose()
        {
            Tx.Dispose(); Log.Dispose(); Data.Dispose();
            Directory.Delete(_directory, recursive: true);
        }
    }
    public static TheoryData<string, bool> Cases
    {
        get
        {
            var cases = new TheoryData<string, bool>();
            foreach (var point in new[] { "append-before", "append-after", "log-before", "log-after", "name-before", "name-after", "data-before", "data-after", "commit-append-before", "commit-append-after", "commit-log-before", "commit-log-after" })
                foreach (var explicitOwner in new[] { false, true }) cases.Add(point, explicitOwner);
            return cases;
        }
    }
    [Theory]
    [MemberData(nameof(Cases))]
    public void IndependentImagesRecoverNamesAndKeepRetryOwnership(string point, bool explicitOwner)
    {
        using var live = new Harness(Image.Empty);
        var tx = live.Tx;
        var id = tx.CreateAndUpdate(11u, 12u);
        tx.SetName(id, "original");
        using var pending = explicitOwner ? tx.BeginTransaction() : null;
        live.Control.Point = point.Replace("commit-", "");
        live.Control.Skip = point.StartsWith("commit-", StringComparison.Ordinal) ? 1 : 0;
        Assert.Contains(live.Control.Point, Assert.Throws<IOException>(() => { tx.SetName(id, "changed 雪|\0\n"); pending?.Commit(); }).Message);
        Assert.Null(live.Control.Point);
        var owner = tx.Journal.Last().TransactionId;
        var image = Assert.IsType<Image>(live.Control.Failed);
        var committed = image.Journal.Contains($"__transactions:commit:{owner:N}");
        var expected = committed ? "changed 雪|\0\n" : "original";
        for (var reopen = 0; reopen < 2; reopen++)
        {
            using var restored = new Harness(image);
            Assert.True(expected == restored.Tx.GetName(id), $"{point}/{explicitOwner}/{reopen}: expected={expected}, actual={restored.Tx.GetName(id)}; input={string.Join('\n', image.Journal)}; recovered={string.Join('\n', restored.Tx.Journal.Select(record => record.Serialize()))}");
            Assert.Equal(11u, restored.Tx.GetSource(id)); Assert.Equal(12u, restored.Tx.GetTarget(id));
            restored.Tx.SetName(77, "following");
            image = restored.Control.Image;
        }
        if (explicitOwner || committed || point.StartsWith("commit-", StringComparison.Ordinal))
        {
            Assert.Throws<InvalidOperationException>(() => tx.SetName(id, "blocked"));
            Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
        }
        tx.Rollback(); tx.Rollback();
        tx.SetName(77, "following");
        // Retry completes an attempted commit; its earlier failure image may
        // still represent an unfinished transaction that recovery must undo.
        Assert.Equal(point.StartsWith("commit-", StringComparison.Ordinal) ? "changed 雪|\0\n" : expected, tx.GetName(id));
    }
}
