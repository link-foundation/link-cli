using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CheckpointDeltaTests
{
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "engine-checkpoint-vectors.json")));
    private static string[] Strings(JsonElement value) => value.EnumerateArray().Select(item => item.GetString()!).ToArray();
    private static MemoryCommitLog Memory(IEnumerable<string> records)
    {
        var raw = new MemoryCommitLog(); foreach (string record in records) raw.Append(record);
        return raw;
    }

    [Fact]
    public void PhysicalDeltaAcceptsIndependentInitialCheckpointVectors()
    {
        using var vectors = Vectors();
        foreach (var value in vectors.RootElement.GetProperty("valid").EnumerateArray())
        {
            var records = Strings(value);
            Assert.Equal(records, new DeltaCommitLog(Memory(records)).Read());
            var appended = new DeltaCommitLog(new MemoryCommitLog());
            foreach (string record in records) appended.Append(record);
            Assert.Equal(records, appended.Read());
        }
    }

    [Fact]
    public void CheckpointCompressionPreservesExactBytesAndChangeBoundaries()
    {
        using var vectors = Vectors(); var fixture = vectors.RootElement.GetProperty("physicalDelta");
        var logical = Strings(fixture.GetProperty("logical")); var physical = Strings(fixture.GetProperty("physical"));
        var raw = new MemoryCommitLog(); var log = new DeltaCommitLog(raw);
        foreach (string record in logical) log.Append(record);
        Assert.Equal(logical, log.Read()); Assert.Equal(physical, raw.Read());
        Assert.Equal(logical, new DeltaCommitLog(raw, 1).Read());
        var zero = Strings(fixture.GetProperty("zeroFollow"));
        Assert.Equal(zero, new DeltaCommitLog(Memory(zero)).Read());
        foreach (var value in fixture.GetProperty("invalid").EnumerateArray())
        {
            var records = Strings(value);
            Assert.Throws<InvalidDataException>(() => new DeltaCommitLog(Memory(records)).Read());
            var appended = new DeltaCommitLog(new MemoryCommitLog());
            foreach (string record in records[..^1]) appended.Append(record);
            var prefix = appended.Read();
            Assert.Throws<InvalidDataException>(() => appended.Append(records[^1]));
            Assert.Equal(prefix, appended.Read());
        }
    }

    [Fact]
    public void CheckpointsComposeWithDeltaHistoryStateEncodingAndImmutablePins()
    {
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var encoding in Enum.GetValues<RevisionEncoding>())
                foreach (int? history in new int?[] { 0, 2, null })
                {
                    var options = new EngineOptions { StateStructure = structure, RevisionEncoding = encoding, HistoryLimit = history };
                    var engine = new LinksEngine<uint>(new MemoryCommitLog(), options);
                    for (int revision = 1; revision <= 6; revision++)
                    {
                        using var tx = engine.Begin();
                        if (revision == 1)
                        {
                            for (uint id = 1; id <= 32; id++) tx.EnsureCreated(id);
                            tx.EnsureCreated(40); tx.DeleteLink(40);
                        }
                        tx.SetName(16, $"雪;|😀 {revision}"); tx.Commit();
                    }
                    var exported = engine.ExportCheckpoint(); var raw = new MemoryCommitLog(); var log = new DeltaCommitLog(raw);
                    foreach (string record in exported) log.Append(record);
                    var copy = new LinksEngine<uint>(log, options);
                    for (ulong revision = 0; revision <= 6; revision++)
                    {
                        bool retained = history is null || revision == 6 || (history == 2 && revision == 5);
                        if (retained) Assert.Equal(engine.SnapshotAt(revision).Rows.ToArray(), copy.SnapshotAt(revision).Rows.ToArray());
                        else Assert.Throws<KeyNotFoundException>(() => copy.SnapshotAt(revision));
                    }
                    var pinned = copy.Snapshot();
                    using (var tx = copy.Begin()) { Assert.Equal(41u, tx.CreateLink(1, 0)); tx.SetName(16, "after checkpoint"); tx.Commit(); }
                    Assert.Equal("雪;|😀 6", pinned.Rows[16].Name); Assert.False(pinned.Rows.ContainsKey(41));
                    Assert.Equal(exported[0], raw.Read()[0]);
                    Assert.StartsWith(encoding == RevisionEncoding.Snapshot ? "delta1|" : "engine2|", raw.Read()[^1]);
                    var reopenedLog = new DeltaCommitLog(raw, 1); var reopened = new LinksEngine<uint>(reopenedLog, options);
                    Assert.Equal(log.Read(), reopenedLog.Read()); Assert.Equal(7ul, reopened.Snapshot().Revision);
                    Assert.True(reopened.Snapshot().Rows.ContainsKey(41));
                }
    }

    [Theory]
    [InlineData("append-before")]
    [InlineData("append-after")]
    [InlineData("flush")]
    public void CheckpointAndFollowingDeltaUncertaintyFenceUntilActualPrefixReopen(string mode)
    {
        using var vectors = Vectors(); var records = Strings(vectors.RootElement.GetProperty("physicalDelta").GetProperty("logical"));
        for (int prefix = 0; prefix <= 1; prefix++)
        {
            var inner = new FaultLog(Memory(records[..prefix]), mode); var log = new DeltaCommitLog(inner);
            Assert.True(log.SupportsDurableFlush);
            if (mode == "flush") { log.Append(records[prefix]); Assert.Throws<IOException>(() => log.Flush()); }
            else Assert.Throws<IOException>(() => log.Append(records[prefix]));
            int calls = inner.Calls;
            Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append(records[prefix])); Assert.Throws<IOException>(() => log.Flush());
            Assert.Equal(calls, inner.Calls);
            var recovered = new DeltaCommitLog(inner.Raw); int expected = prefix + (mode == "append-before" ? 0 : 1);
            Assert.Equal(records[..expected], recovered.Read());
            foreach (string record in records[expected..]) recovered.Append(record);
            Assert.Equal(records, recovered.Read());
        }
    }

    private sealed class FaultLog(MemoryCommitLog raw, string mode) : ICommitLog
    {
        public readonly MemoryCommitLog Raw = raw;
        public int Calls;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<string> Read() { Calls++; return Raw.Read(); }
        public void Append(string record)
        {
            Calls++; if (mode == "append-before") throw new IOException(mode);
            Raw.Append(record); if (mode == "append-after") throw new IOException(mode);
        }
        public void Flush() { Calls++; if (mode == "flush") throw new IOException(mode); }
    }

    [Fact]
    public void CheckpointReplicationAcceptsBothPhysicalCompositionOrders()
    {
        using var vectors = Vectors(); var logical = Strings(vectors.RootElement.GetProperty("physicalDelta").GetProperty("logical"));
        foreach (bool outside in new[] { false, true })
        {
            var primary = new MemoryCommitLog(); var follower = new MemoryCommitLog();
            var replica = new ReplicatedCommitLog(outside ? primary : new DeltaCommitLog(primary, 1),
                new ICommitLog[] { outside ? follower : new DeltaCommitLog(follower) }, 2);
            ICommitLog log = outside ? new DeltaCommitLog(replica) : replica;
            foreach (string record in logical) { log.Append(record); log.Flush(); }
            Assert.Equal(2, replica.Synchronize());
            Assert.Equal(logical, new DeltaCommitLog(primary).Read()); Assert.Equal(logical, new DeltaCommitLog(follower).Read());
            if (outside) Assert.Equal(primary.Read(), follower.Read());
            else Assert.NotEqual(primary.Read(), follower.Read());
        }
    }
}
