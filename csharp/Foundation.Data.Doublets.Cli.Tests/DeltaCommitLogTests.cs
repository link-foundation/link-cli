using Foundation.Data.Doublets.Cli.Engine;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests;

public class DeltaCommitLogTests
{
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "engine-delta-vectors.json")));
    private static string[] Strings(JsonElement value) => value.EnumerateArray().Select(item => item.GetString()!).ToArray();
    private static MemoryCommitLog Memory(IEnumerable<string> records)
    {
        var log = new MemoryCommitLog();
        foreach (var record in records) log.Append(record);
        return log;
    }

    [Fact]
    public void SharedWireSequencesReplayExactlyAndRejectCorruption()
    {
        using var vectors = Vectors();
        foreach (var sequence in vectors.RootElement.GetProperty("validSequences").EnumerateArray())
        {
            var log = new DeltaCommitLog(Memory(Strings(sequence.GetProperty("physical"))));
            Assert.Equal(Strings(sequence.GetProperty("logical")), log.Read());
        }
        foreach (var sequence in vectors.RootElement.GetProperty("invalidSequences").EnumerateArray())
        {
            var log = new DeltaCommitLog(Memory(Strings(sequence.GetProperty("records"))));
            Assert.Throws<InvalidDataException>(() => log.Read());
        }
    }

    [Fact]
    public void PortableDeltasAndCheckpointFallbackComposeWithTheEngine()
    {
        using var vectors = Vectors();
        var sequence = vectors.RootElement.GetProperty("validSequences")[0];
        var logical = Strings(sequence.GetProperty("logical"));
        var physical = new MemoryCommitLog();
        var log = new DeltaCommitLog(physical);
        foreach (var record in logical) { log.Append(record); log.Flush(); }
        Assert.Equal(Strings(sequence.GetProperty("physical")), physical.Read());
        Assert.False(log.SupportsDurableFlush);
        var engine = new LinksEngine<uint>(new DeltaCommitLog(physical), new() { HistoryLimit = null });
        Assert.Equal("雪;|😀", engine.SnapshotAt(2).Rows[100].Name);
        using var tx = engine.Begin(); tx.SetName(100, "after reopen"); tx.Commit();
        var recovered = new LinksEngine<uint>(new DeltaCommitLog(physical));
        Assert.Equal("after reopen", recovered.Snapshot().Rows[100].Name);
        Assert.Throws<ArgumentOutOfRangeException>(() => new DeltaCommitLog(physical, 0));
        var plain = new MemoryCommitLog(); var fullOnly = new DeltaCommitLog(plain, 1);
        foreach (var record in logical) fullOnly.Append(record);
        Assert.Equal(logical, plain.Read());
        var periodic = new MemoryCommitLog(); var bounded = new DeltaCommitLog(periodic, 2);
        foreach (var record in logical) bounded.Append(record);
        Assert.StartsWith("delta1|", periodic.Read()[1]);
        Assert.StartsWith("engine1|", periodic.Read()[2]);
    }

    [Fact]
    public void InvalidInputsAndTransientReadsNeverAppend()
    {
        Assert.Throws<ArgumentNullException>(() => new DeltaCommitLog(null!));
        using var vectors = Vectors();
        var inner = new FaultLog(); var log = new DeltaCommitLog(inner, uint.MaxValue);
        Assert.Throws<InvalidDataException>(() => log.Append(null!));
        foreach (var sequence in vectors.RootElement.GetProperty("invalidSequences").EnumerateArray())
        {
            var records = Strings(sequence.GetProperty("records"));
            if (records.Length == 1) Assert.Throws<InvalidDataException>(() => log.Append(records[0]));
        }
        Assert.Empty(inner.Raw.Read());
        inner.Records = null;
        Assert.Throws<InvalidDataException>(() => log.Read());
        inner.Records = new[] { (string)null! };
        Assert.Throws<InvalidDataException>(() => log.Read());
        inner.Mode = "read";
        Assert.Throws<IOException>(() => log.Read());
        inner.Mode = ""; inner.Records = Array.Empty<string>();
        Assert.Empty(log.Read());
        var small = vectors.RootElement.GetProperty("validSequences").EnumerateArray().Last();
        foreach (var record in Strings(small.GetProperty("logical"))) log.Append(record);
        Assert.Equal(Strings(small.GetProperty("physical")), inner.Raw.Read());
        Assert.Throws<InvalidDataException>(() => log.Append(inner.Raw.Read()[1]));
        var resumed = new DeltaCommitLog(Memory(Strings(vectors.RootElement.GetProperty("validSequences")[0].GetProperty("physical")).Take(2)));
        resumed.Append(vectors.RootElement.GetProperty("validSequences")[0].GetProperty("logical")[2].GetString()!);
        Assert.Equal(3, resumed.Read().Count);
        var unavailable = new DeltaCommitLog(new FaultLog { Mode = "read" });
        Assert.Throws<IOException>(() => unavailable.Append(inner.Raw.Read()[0]));
    }

    [Theory]
    [InlineData("append-before")]
    [InlineData("append-after")]
    [InlineData("flush")]
    public void UncertainMutationsFenceEveryOperationUntilReopen(string mode)
    {
        using var vectors = Vectors();
        string record = vectors.RootElement.GetProperty("validSequences")[0].GetProperty("logical")[0].GetString()!;
        var inner = new FaultLog { Mode = mode }; var log = new DeltaCommitLog(inner);
        Assert.True(log.SupportsDurableFlush);
        if (mode == "flush") { log.Append(record); Assert.Throws<IOException>(() => log.Flush()); }
        else Assert.Throws<IOException>(() => log.Append(record));
        int calls = inner.Calls;
        Assert.Throws<IOException>(() => log.Read());
        Assert.Throws<IOException>(() => log.Append(record));
        Assert.Throws<IOException>(() => log.Flush());
        Assert.Equal(calls, inner.Calls);
        var recovered = new DeltaCommitLog(inner.Raw);
        Assert.Equal(mode == "append-before" ? 0 : 1, recovered.Read().Count);
    }

    [Fact]
    public void DurableReplicationCopiesPhysicalDeltasAndPreservesHistory()
    {
        var directory = Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString());
        Directory.CreateDirectory(directory);
        var primaryPath = Path.Combine(directory, "primary.log"); var replicaPath = Path.Combine(directory, "replica.log");
        try
        {
            using var vectors = Vectors();
            var logical = Strings(vectors.RootElement.GetProperty("validSequences")[0].GetProperty("logical"));
            using (var primary = new FileCommitLog(primaryPath))
            using (var replica = new FileCommitLog(replicaPath))
            {
                var physical = new ReplicatedCommitLog(primary, new ICommitLog[] { replica }, 2);
                var log = new DeltaCommitLog(physical);
                Assert.True(log.SupportsDurableFlush);
                foreach (var record in logical) { log.Append(record); log.Flush(); }
            }
            Assert.Equal(File.ReadAllBytes(primaryPath), File.ReadAllBytes(replicaPath));
            using var recoveredLog = new FileCommitLog(replicaPath);
            Assert.StartsWith("delta1|", recoveredLog.Read()[1]);
            var recovered = new LinksEngine<uint>(new DeltaCommitLog(recoveredLog), new() { RequireDurableFlush = true, HistoryLimit = null });
            Assert.Null(recovered.SnapshotAt(1).Rows[100].Name);
            Assert.Equal("雪;|😀", recovered.SnapshotAt(2).Rows[100].Name);
        }
        finally { Directory.Delete(directory, true); }
    }

    private sealed class FaultLog : ICommitLog
    {
        public readonly MemoryCommitLog Raw = new();
        public string Mode = "";
        public IReadOnlyList<string>? Records = Array.Empty<string>();
        public int Calls;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<string> Read()
        {
            Calls++;
            if (Mode == "read") throw new IOException("read unavailable");
            return Records!;
        }
        public void Append(string record)
        {
            Calls++;
            if (Mode == "append-before") throw new IOException("append before");
            Raw.Append(record);
            if (Mode == "append-after") throw new IOException("append after");
        }
        public void Flush() { Calls++; if (Mode == "flush") throw new IOException("flush unavailable"); }
    }

    [Fact]
    public void ReplicasChooseIndependentPhysicalCheckpointsAndCompareLogicalPrefixes()
    {
        using var vectors = Vectors();
        var logical = Strings(vectors.RootElement.GetProperty("validSequences")[0].GetProperty("logical"));
        var primary = new MemoryCommitLog(); var follower = new MemoryCommitLog();
        var log = new ReplicatedCommitLog(new DeltaCommitLog(primary, 1), new ICommitLog[] { new DeltaCommitLog(follower) }, 2);
        foreach (var record in logical) { log.Append(record); log.Flush(); }
        Assert.Equal(2, log.Synchronize());
        Assert.Equal(logical, new DeltaCommitLog(primary).Read());
        Assert.Equal(logical, new DeltaCommitLog(follower).Read());
        Assert.NotEqual(primary.Read(), follower.Read());
    }

    [Fact]
    public void FailedRereadInvalidatesCachedAdmissionBeforeAnotherAppend()
    {
        using var vectors = Vectors();
        var sequence = vectors.RootElement.GetProperty("validSequences")[0];
        var prefix = Strings(sequence.GetProperty("physical")).Take(2).ToArray();
        var inner = new FaultLog { Records = prefix };
        var log = new DeltaCommitLog(inner);
        Assert.Equal(2, log.Read().Count);
        inner.Records = null;
        Assert.Throws<InvalidDataException>(() => log.Read());
        var next = sequence.GetProperty("logical")[2].GetString()!;
        Assert.Throws<InvalidDataException>(() => log.Append(next));
        Assert.Empty(inner.Raw.Read());
        inner.Records = prefix;
        log.Append(next);
        Assert.Single(inner.Raw.Read());
    }
}
