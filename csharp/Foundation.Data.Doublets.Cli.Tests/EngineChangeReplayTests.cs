using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineChangeReplayTests
{
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "engine-change-vectors.json")));
    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void IndependentChangeAndMixedRecordsPreserveNamesRowsHistoryAndAllocator(StateStructure structure)
    {
        using var fixtures = Vectors();
        foreach (var kind in new[] { "valid", "mixed" })
        {
            var log = new MemoryCommitLog();
            foreach (var row in fixtures.RootElement.GetProperty(kind).EnumerateArray()) log.Append(row.GetString()!);
            var original = log.Read().ToArray(); var options = new EngineOptions { StateStructure = structure, HistoryLimit = null };
            var engine = new LinksEngine<uint>(log, options); var first = engine.SnapshotAt(1);
            Assert.Equal("雪;|😀\0\n", first.Rows[1].Name); Assert.Equal("", first.Rows[2].Name);
            var second = engine.SnapshotAt(2); Assert.False(second.Rows.ContainsKey(2));
            Assert.Equal(2U, second.Rows[1].Link.Source); Assert.Null(second.Rows[1].Name);
            var current = engine.Snapshot(); Assert.Equal(4UL, current.Revision); Assert.Equal(2, current.Rows.Count);
            Assert.Equal("", current.Rows[uint.MaxValue].Name); Assert.Equal(uint.MaxValue, current.Rows[uint.MaxValue].Link.Target);
            Assert.Equal(original, log.Read());
            using (var following = engine.Begin()) { following.SetName(1, "following"); Assert.Equal(5UL, following.Commit()); }
            Assert.Equal("雪;|😀\0\n", first.Rows[1].Name); Assert.StartsWith("engine1|5|4294967297|", log.Read().Last());
            var reopened = new LinksEngine<uint>(log, options); Assert.Equal("following", reopened.Snapshot().Rows[1].Name);
            using var exhausted = reopened.Begin(); Assert.Throws<OverflowException>(() => exhausted.CreateLink(0, 0)); exhausted.Rollback();
            Assert.Equal(5UL, reopened.Snapshot().Revision);
        }
    }

    private sealed class ReadOnlyLog(string[] records) : ICommitLog
    {
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => records;
        public void Append(string record) => throw new InvalidOperationException("Recovery wrote an invalid journal.");
        public void Flush() => throw new InvalidOperationException("Recovery flushed an invalid journal.");
    }
    [Fact]
    public void PhysicalDeltaDecoratorPreservesChangesAndResetsSnapshotCompressionBases()
    {
        using var fixtures = Vectors(); var data = fixtures.RootElement.GetProperty("physicalMixed");
        var logical = data.GetProperty("logical").EnumerateArray().Select(row => row.GetString()!).ToArray();
        var expected = data.GetProperty("physical").EnumerateArray().Select(row => row.GetString()!).ToArray();
        var raw = new MemoryCommitLog(); var log = new DeltaCommitLog(raw);
        foreach (var record in logical) log.Append(record);
        Assert.Equal(logical, log.Read()); Assert.Equal(expected, raw.Read());
        var reopened = new LinksEngine<uint>(new DeltaCommitLog(raw));
        Assert.Equal(5UL, reopened.Snapshot().Revision); Assert.Equal(31, reopened.Snapshot().Rows.Count);
        Assert.False(reopened.Snapshot().Rows.ContainsKey(2)); Assert.Equal(4U, reopened.Snapshot().Rows[1].Link.Source);
        foreach (var fixture in fixtures.RootElement.GetProperty("invalidPhysical").EnumerateArray())
        {
            var records = fixture.GetProperty("records").EnumerateArray().Select(row => row.GetString()!).ToArray();
            var invalidRaw = new MemoryCommitLog(); foreach (var record in records) invalidRaw.Append(record);
            Assert.Throws<InvalidDataException>(() => new DeltaCommitLog(invalidRaw).Read());
            var appendingRaw = new MemoryCommitLog(); var appending = new DeltaCommitLog(appendingRaw);
            foreach (var record in records[..^1]) appending.Append(record);
            Assert.Throws<InvalidDataException>(() => appending.Append(records[^1]));
            Assert.Equal(records[..^1], appendingRaw.Read());
        }
    }
    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void MalformedChangesRejectWithoutRewritingTheSuppliedJournal(StateStructure structure)
    {
        using var fixtures = Vectors();
        foreach (var fixture in fixtures.RootElement.GetProperty("invalidU32").EnumerateArray())
        {
            var records = fixture.GetProperty("records").EnumerateArray().Select(row => row.GetString()!).ToArray();
            var saved = records.ToArray(); var log = new ReadOnlyLog(records);
            Assert.Throws<InvalidDataException>(() => new LinksEngine<uint>(log, new() { StateStructure = structure }));
            Assert.Equal(saved, log.Read());
        }
    }
}
