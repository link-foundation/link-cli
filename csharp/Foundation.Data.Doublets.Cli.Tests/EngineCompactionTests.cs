using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineCompactBefore
{
    private sealed class ReplacingLog : IReplaceableCommitLog
    {
        public bool SupportsDurableFlush => false;
        private string[] _rows = Array.Empty<string>();
        public IReadOnlyList<string> Read() => _rows.ToArray();
        public void Append(string row) => _rows = _rows.Append(row).ToArray();
        public void Flush() { }
        public bool Replace(IReadOnlyList<string> rows)
        { bool changed = !_rows.SequenceEqual(rows); _rows = rows.ToArray(); return changed; }
    }
    [Fact]
    public void CompactPublishesInitialCheckpointAndReportsIdenticalRepetition()
    {
        var log = new ReplacingLog(); var engine = new LinksEngine<uint>(log);
        var expected = engine.ExportCheckpoint();
        Assert.True(engine.CompactJournal()); Assert.Equal(expected, log.Read());
        Assert.False(engine.CompactJournal());
    }
}
