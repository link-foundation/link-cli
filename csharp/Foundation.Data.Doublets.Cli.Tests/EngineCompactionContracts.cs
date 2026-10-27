using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineCompactContracts
{
    private sealed class ReplacingLog : IReplaceableCommitLog
    {
        public bool SupportsDurableFlush => true;
        public bool SupportsReplacement { get; set; } = true;
        public List<string> Live { get; private set; } = new();
        public string[] Durable { get; private set; } = Array.Empty<string>();
        public bool? Fault { get; set; }
        public int Replacements { get; private set; }
        public IReadOnlyList<string> Read() => Live.ToArray();
        public void Append(string record) => Live.Add(record);
        public void Flush() => Durable = Live.ToArray();
        public bool Replace(IReadOnlyList<string> records)
        {
            Replacements++;
            if (Fault == false) throw new IOException("before publication");
            bool changed = !Live.SequenceEqual(records);
            Live = records.ToList(); Durable = records.ToArray();
            if (Fault == true) throw new IOException("after publication");
            return changed;
        }
        public ReplacingLog Recovered() => new() { Live = Durable.ToList(), Durable = Durable.ToArray() };
    }
    [Fact]
    public void ReplacementRetainsHistoryPrivateReservationsPinsAndFollowingGroups()
    {
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var encoding in Enum.GetValues<RevisionEncoding>())
                foreach (int? history in new int?[] { 0, 2, null })
                    foreach (bool delta in new[] { false, true })
                    {
                        var options = new EngineOptions { Isolation = IsolationLevel.Snapshot, StateStructure = structure, RevisionEncoding = encoding, HistoryLimit = history, RequireDurableFlush = true };
                        IReplaceableCommitLog log = delta ? new DeltaCommitLog(new ReplacingLog()) : new ReplacingLog();
                        var engine = new LinksEngine<uint>(log, options);
                        Assert.True(engine.CompactJournal()); Assert.False(engine.CompactJournal());
                        EngineSnapshot<uint>? pin = null;
                        for (int revision = 1; revision <= 6; revision++)
                        {
                            using var tx = engine.Begin();
                            if (revision == 1) tx.CreateLink(0, 0);
                            if (revision == 3) { tx.EnsureCreated(7); tx.DeleteLink(7); }
                            tx.SetName(1, $"雪 {new string('x', 256)} {revision}"); tx.Commit();
                            if (revision == 1) pin = engine.Snapshot();
                        }
                        var retained = new Dictionary<ulong, EngineSnapshot<uint>>();
                        for (ulong revision = 0; revision <= 6; revision++)
                            if (history is null || revision == 6 || (history == 2 && revision == 5)) retained.Add(revision, engine.SnapshotAt(revision));
                        using var pending = engine.Begin(); pending.EnsureCreated(99);
                        var expected = engine.ExportCheckpoint();
                        Assert.Equal(!log.Read().SequenceEqual(expected), engine.CompactJournal()); Assert.Equal(expected, log.Read()); Assert.False(engine.CompactJournal());
                        foreach (var (revision, snapshot) in retained) Assert.Equal(snapshot.Rows.ToArray(), engine.SnapshotAt(revision).Rows.ToArray());
                        var copyLog = new MemoryCommitLog(); foreach (string entry in expected) copyLog.Append(entry);
                        var copy = new LinksEngine<uint>(copyLog, options with { RequireDurableFlush = false });
                        using (var tx = copy.Begin()) Assert.Equal(8u, tx.CreateLink(0, 0));
                        Assert.Equal(6ul, engine.Snapshot().Revision); Assert.False(engine.Snapshot().Rows.ContainsKey(99));
                        pending.Commit();
                        using var first = engine.Begin(); using var second = engine.Begin();
                        Assert.Equal(100u, first.CreateLink(1, 0)); Assert.Equal(101u, second.CreateLink(0, 1));
                        Assert.All(engine.CommitGroup(new[] { first, second }), item => Assert.Equal(GroupCommitStatus.Committed, item.Status));
                        Assert.Equal(9ul, engine.Snapshot().Revision); Assert.False(pin!.Rows.ContainsKey(99)); Assert.EndsWith(" 1", pin.Rows[1].Name);
                        Assert.Equal(!log.Read().SequenceEqual(engine.ExportCheckpoint()), engine.CompactJournal()); Assert.False(engine.CompactJournal());
                        var restored = new LinksEngine<uint>(log, options);
                        Assert.Equal(engine.Snapshot().Rows.ToArray(), restored.Snapshot().Rows.ToArray());
                        using var following = restored.Begin(); Assert.Equal(102u, following.CreateLink(0, 0));
                    }
    }
    [Fact]
    public void UnsupportedReplacementLeavesEngineAvailableForFollowingCommits()
    {
        var log = new ReplacingLog { SupportsReplacement = false }; var engine = new LinksEngine<uint>(log);
        Assert.Throws<NotSupportedException>(() => engine.CompactJournal()); Assert.Equal(0, log.Replacements);
        using var tx = engine.Begin(); tx.CreateLink(0, 0); tx.Commit(); Assert.Equal(1ul, engine.Snapshot().Revision);
        Assert.Throws<NotSupportedException>(() => new LinksEngine<uint>(new MemoryCommitLog()).CompactJournal());
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void BeforeAndAfterPublicationErrorsFenceEngineAndLiveSessionsUntilReopen(bool after)
    {
        var log = new ReplacingLog(); var engine = new LinksEngine<uint>(log);
        using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
        var pin = engine.Snapshot(); using var pending = engine.Begin(); pending.CreateLink(0, 0); log.Fault = after;
        Assert.Throws<IOException>(() => engine.CompactJournal());
        Assert.Throws<InvalidOperationException>(() => engine.Snapshot()); Assert.Throws<InvalidOperationException>(() => engine.Begin());
        Assert.Throws<InvalidOperationException>(() => engine.ExportCheckpoint()); Assert.Throws<InvalidOperationException>(() => engine.CompactJournal());
        Assert.Throws<InvalidOperationException>(() => pending.Commit()); Assert.Single(pin.Rows);
        Assert.Equal(after, log.Durable[0].StartsWith("engine3|", StringComparison.Ordinal));
        var restored = new LinksEngine<uint>(log.Recovered()); Assert.Equal(pin.Rows.ToArray(), restored.Snapshot().Rows.ToArray());
        using (var next = restored.Begin()) { Assert.Equal(2u, next.CreateLink(0, 0)); next.Commit(); }
        Assert.True(restored.CompactJournal()); Assert.False(restored.CompactJournal());
    }
    [Fact]
    public void DeltaReplacementPreparesExactVectorsValidatesBeforeIoAndResumes()
    {
        using var vectors = System.Text.Json.JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/engine-checkpoint-vectors.json")));
        string[] logical = vectors.RootElement.GetProperty("physicalDelta").GetProperty("logical").EnumerateArray().Select(x => x.GetString()!).ToArray();
        string[] physical = vectors.RootElement.GetProperty("physicalDelta").GetProperty("physical").EnumerateArray().Select(x => x.GetString()!).ToArray();
        var inner = new ReplacingLog(); var log = new DeltaCommitLog(inner);
        Assert.True(log.SupportsReplacement); Assert.True(log.Replace(logical[..2])); Assert.Equal(physical[..2], inner.Read());
        Assert.False(log.Replace(logical[..2])); foreach (string record in logical[2..]) log.Append(record); log.Flush();
        Assert.Equal(physical, inner.Read()); Assert.Equal(logical, log.Read());
        int calls = inner.Replacements;
        Assert.Throws<ArgumentNullException>(() => log.Replace(null!));
        foreach (string[] invalid in new[] { new[] { "bad" }, new[] { logical[0], "bad" }, new[] { logical[0], logical[0] }, new[] { (string)null! } })
        {
            Assert.Throws<InvalidDataException>(() => log.Replace(invalid)); Assert.Equal(calls, inner.Replacements); Assert.Equal(logical, log.Read());
        }
        Assert.True(log.Replace(Array.Empty<string>())); Assert.False(log.Replace(Array.Empty<string>()));
        Assert.True(log.Replace(logical)); Assert.Equal(physical, inner.Read());
        var unavailable = new ReplacingLog { SupportsReplacement = false }; var unsupported = new DeltaCommitLog(unavailable);
        Assert.False(unsupported.SupportsReplacement); Assert.Throws<NotSupportedException>(() => unsupported.Replace(Array.Empty<string>())); Assert.Equal(0, unavailable.Replacements);
        foreach (string record in logical) unsupported.Append(record); Assert.Equal(logical, unsupported.Read());
        var ordinary = new DeltaCommitLog(new MemoryCommitLog());
        Assert.False(ordinary.SupportsReplacement);
        Assert.Throws<NotSupportedException>(() => ordinary.Replace(logical));
        foreach (string record in logical) ordinary.Append(record);
        ordinary.Flush(); Assert.Equal(logical, ordinary.Read());
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void DeltaReplacementErrorsFenceCodecUntilReopenCompleteOldOrNewStream(bool after)
    {
        using var vectors = System.Text.Json.JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/engine-checkpoint-vectors.json")));
        string[] logical = vectors.RootElement.GetProperty("physicalDelta").GetProperty("logical").EnumerateArray().Select(x => x.GetString()!).ToArray();
        var inner = new ReplacingLog(); var log = new DeltaCommitLog(inner); log.Replace(logical[..1]); inner.Fault = after;
        Assert.Throws<IOException>(() => log.Replace(logical)); int calls = inner.Replacements;
        Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append(logical[1]));
        Assert.Throws<IOException>(() => log.Flush()); Assert.Throws<IOException>(() => log.Replace(Array.Empty<string>())); Assert.Equal(calls, inner.Replacements);
        var reopened = new DeltaCommitLog(inner.Recovered()); Assert.Equal(after ? logical : logical[..1], reopened.Read()); Assert.Equal(!after, reopened.Replace(logical));
    }

    [Fact]
    public void CompactionPreservesLocksAdmissionAndStaleDependencies()
    {
        foreach (var concurrency in Enum.GetValues<ConcurrencyControl>())
            foreach (var isolation in Enum.GetValues<IsolationLevel>())
            {
                var engine = new LinksEngine<uint>(new ReplacingLog(), new() { Concurrency = concurrency, Isolation = isolation });
                using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
                using var first = engine.Begin(); first.SetName(1, "held"); engine.CompactJournal();
                if (concurrency == ConcurrencyControl.Exclusive)
                { Assert.Throws<InvalidOperationException>(() => engine.Begin()); first.Commit(); }
                else
                {
                    using var second = engine.Begin();
                    if (concurrency == ConcurrencyControl.Pessimistic) Assert.Throws<InvalidOperationException>(() => second.SetName(1, "contender"));
                    else second.SetName(1, "contender");
                    first.Commit(); engine.CompactJournal();
                    if (concurrency == ConcurrencyControl.Optimistic && isolation != IsolationLevel.ReadCommitted)
                        Assert.Throws<InvalidOperationException>(() => second.Commit());
                    second.Rollback();
                }
                using var following = engine.Begin(); following.SetName(1, "following"); following.Commit();
                Assert.Equal("following", engine.Snapshot().Rows[1].Name);
            }
        foreach (var concurrency in new[] { ConcurrencyControl.Optimistic, ConcurrencyControl.Pessimistic })
        {
            var engine = new LinksEngine<uint>(new ReplacingLog(), new() { Concurrency = concurrency, SerializableValidation = SerializableValidation.ReadSet });
            using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
            using var stale = engine.Begin(); Assert.Null(stale.GetName(1));
            using (var writer = engine.Begin()) { writer.SetName(1, "changed"); writer.Commit(); }
            engine.CompactJournal(); stale.CreateLink(0, 0); Assert.Throws<InvalidOperationException>(() => stale.Commit());
        }
    }
    [Fact]
    public void CompactedParentAndBranchResumeExactRetainedDescriptorAndMergeAllocator()
    {
        foreach (int? history in new int?[] { null, 2 }) foreach (bool delta in new[] { false, true })
            {
                var parent = new LinksEngine<uint>(new ReplacingLog());
                using (var seed = parent.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
                var options = new EngineOptions { HistoryLimit = history };
                IReplaceableCommitLog log = delta ? new DeltaCommitLog(new ReplacingLog()) : new ReplacingLog();
                var branch = parent.Fork(log, options); string descriptor = branch.MergeBaseRecord;
                using (var edit = branch.Engine.Begin()) { edit.EnsureCreated(7); edit.DeleteLink(7); edit.SetName(1, "compacted branch 雪"); edit.Commit(); }
                branch.Engine.CompactJournal(); parent.CompactJournal(); Assert.Equal(descriptor, branch.MergeBaseRecord);
                var resumed = parent.ResumeBranch(log, descriptor, options); Assert.Equal(2ul, resumed.Merge());
                Assert.Equal("compacted branch 雪", parent.Snapshot().Rows[1].Name);
                using (var following = parent.Begin()) { Assert.Equal(8u, following.CreateLink(0, 0)); following.Commit(); }
                parent.CompactJournal(); Assert.False(parent.CompactJournal());
            }
    }

}
