using System.Text.Json;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionRowUndoWideTests
{
    private const ulong High = 1UL << 32;
    private sealed record Image(Link<ulong>[] Rows, Dictionary<ulong, string> Names);

    // The base owns only tiny unused mappings. All logical rows use this sparse
    // map; the bounded creator prevents an accidental dense allocation probe.
    private sealed class Sparse(string path, Image image) : NamedTypesDecorator<ulong>(path)
    {
        private readonly SortedDictionary<ulong, Link<ulong>> _rows = new(image.Rows.ToDictionary(row => row.Index));
        private readonly Dictionary<ulong, string> _names = new(image.Names);
        private ulong _next = High;
        private int _creates;
        internal Image Capture() => new(_rows.Values.ToArray(), new(_names));
        private Link<ulong>[] Matching(IList<ulong>? restriction) => _rows.Values.Where(row =>
            restriction is null || restriction.Select((part, position) => part == Constants.Any || part ==
                (position switch { 0 => row.Index, 1 => row.Source, _ => row.Target })).All(match => match)).ToArray();
        public override ulong Count(IList<ulong>? restriction) => (ulong)Matching(restriction).Length;
        public override ulong Each(IList<ulong>? restriction, ReadHandler<ulong>? handler)
        {
            foreach (var row in Matching(restriction))
                if (handler?.Invoke((IList<ulong>)row) == Constants.Break) return Constants.Break;
            return Constants.Continue;
        }
        public override ulong Create(IList<ulong>? substitution, WriteHandler<ulong>? handler)
        {
            if (++_creates > 8) throw new InvalidOperationException("Sparse fixture allocation bound exceeded.");
            while (_rows.ContainsKey(_next)) _next++;
            var row = new Link<ulong>(_next++, 0, 0); _rows.Add(row.Index, row);
            return handler?.Invoke(null, (IList<ulong>)row) ?? Constants.Continue;
        }
        public override ulong Update(IList<ulong>? restriction, IList<ulong>? substitution, WriteHandler<ulong>? handler)
        {
            var id = restriction![0]; var before = _rows[id]; var after = new Link<ulong>(id, substitution![1], substitution[2]);
            _rows[id] = after;
            return handler?.Invoke((IList<ulong>)before, (IList<ulong>)after) ?? Constants.Continue;
        }
        public override ulong Delete(IList<ulong>? restriction, WriteHandler<ulong>? handler)
        {
            var id = restriction![0]; var before = _rows[id]; _rows.Remove(id);
            return handler?.Invoke((IList<ulong>)before, null) ?? Constants.Continue;
        }
        public override IReadOnlyDictionary<ulong, string> CaptureNames() => new Dictionary<ulong, string>(_names);
        public override string? GetName(ulong id) => _names.GetValueOrDefault(id);
        public override ulong SetName(ulong id, string name) { _names[id] = name; return id; }
        public override void RemoveName(ulong id) => _names.Remove(id);
    }

    [Fact]
    public void WideImagesRecoverAndRollbackThroughSparseAdapters()
    {
        var directory = Directory.CreateTempSubdirectory("sparse-image-").FullName;
        try
        {
            using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "transaction-undo-vectors.json")));
            using (var log = new NamedTypesDecorator<ulong>(Path.Combine(directory, "log")))
                log.SetName(log.CreateAndUpdate(0UL, 0UL), "__transactions:transition:" + vectors.RootElement.GetProperty("wide").GetString());
            var image = new Image([], []);
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new Sparse(Path.Combine(directory, "data"), image);
                using var log = new NamedTypesDecorator<ulong>(Path.Combine(directory, "log"));
                using var tx = new TransactionsDecorator<ulong>(data, log, RowUndoPolicy.WriteAheadImage, flushData: () => image = data.Capture());
                Assert.Equal(new[] { new Link<ulong>(High, High + 1, 0) }, data.Capture().Rows);
                Assert.Equal(new Dictionary<ulong, string> { [0] = "" }, data.Capture().Names);
                using var pending = tx.BeginTransaction();
                tx.Update(new[] { High }, new[] { High, High + 2, High }, null); tx.SetName(High, "wide 雪");
                pending.Rollback();
                Assert.Equal(new[] { new Link<ulong>(High, High + 1, 0) }, data.Capture().Rows);
                Assert.Equal(new Dictionary<ulong, string> { [0] = "" }, data.Capture().Names);
                if (reopen == 1)
                {
                    Assert.Equal(High + 1, tx.CreateAndUpdate(High, High + 1));
                    Assert.Equal(2, data.Capture().Rows.Length);
                }
            }
        }
        finally { Directory.Delete(directory, true); }
    }
}
