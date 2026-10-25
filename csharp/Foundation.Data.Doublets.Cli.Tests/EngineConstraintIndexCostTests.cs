using System.Collections;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineConstraintIndexCostTests
{
    private static EngineOptions Options(params EngineConstraint[] constraints) => new()
    { ConstraintValidation = ConstraintValidation.Indexed, Constraints = constraints };

    [Fact]
    public void IndexedConstraintsPreserveAdmissionAndReadValidation()
    {
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var encoding in Enum.GetValues<RevisionEncoding>())
                foreach (var concurrency in Enum.GetValues<ConcurrencyControl>())
                    foreach (var isolation in Enum.GetValues<IsolationLevel>())
                        foreach (var validation in Enum.GetValues<SerializableValidation>())
                        {
                            if (isolation != IsolationLevel.Serializable && validation == SerializableValidation.ReadSet) continue;
                            var engine = new LinksEngine<uint>(new MemoryCommitLog(), Options(Enum.GetValues<EngineConstraint>()) with
                            { StateStructure = structure, RevisionEncoding = encoding, Concurrency = concurrency, Isolation = isolation, SerializableValidation = validation });
                            using (var seed = engine.Begin())
                            { for (uint id = 1; id <= 3; id++) { seed.CreateLink(id, id); seed.SetName(id, $"name {id}"); } seed.Commit(); }
                            using var reader = engine.Begin(); Assert.NotNull(reader.GetLink(1));
                            if (concurrency == ConcurrencyControl.Exclusive) Assert.Throws<InvalidOperationException>(() => engine.Begin());
                            else { using var writer = engine.Begin(); writer.SetName(1, "concurrent"); writer.Commit(); }
                            reader.SetName(2, "reader");
                            if (concurrency == ConcurrencyControl.Exclusive || isolation != IsolationLevel.Serializable) reader.Commit();
                            else Assert.Throws<InvalidOperationException>(() => reader.Commit());
                            using (var bad = engine.Begin())
                            { bad.SetName(3, engine.Snapshot().Rows[1].Name!); Assert.Throws<InvalidOperationException>(() => bad.Commit()); }
                            using var following = engine.Begin(); following.SetName(3, "following"); following.Commit();
                        }
    }

    [Fact]
    public void OnlySelectedConstraintsRetainIndexes()
    {
        var rows = new Dictionary<uint, EngineRow<uint>>();
        Assert.Null(ConstraintIndex<uint>.Build(new() { Constraints = Enum.GetValues<EngineConstraint>() }, rows));
        Assert.Null(ConstraintIndex<uint>.Build(Options(), rows));
        for (int mask = 1; mask < 8; mask++)
        {
            var selected = Enum.GetValues<EngineConstraint>().Where((_, bit) => (mask & (1 << bit)) != 0).ToArray();
            var index = ConstraintIndex<uint>.Build(Options(selected), rows)!;
            Assert.Equal((mask & 1) != 0, index.References is not null);
            Assert.Equal((mask & 2) != 0, index.Pairs is not null);
            Assert.Equal((mask & 4) != 0, index.Names is not null);
        }
    }

    // Every enumeration throws. Successful preparation proves the bounded
    // edit uses point lookups; these counters do not alter production code.
    private sealed class PointRows(IReadOnlyDictionary<uint, EngineRow<uint>> rows) : IReadOnlyDictionary<uint, EngineRow<uint>>
    {
        internal int Lookups;
        public EngineRow<uint> this[uint key] => throw new InvalidOperationException("Unexpected indexer.");
        public IEnumerable<uint> Keys => throw new InvalidOperationException("Unexpected scan.");
        public IEnumerable<EngineRow<uint>> Values => throw new InvalidOperationException("Unexpected scan.");
        public int Count => throw new InvalidOperationException("Unexpected count.");
        public bool ContainsKey(uint key) { Lookups++; return rows.ContainsKey(key); }
        public bool TryGetValue(uint key, out EngineRow<uint> value) { Lookups++; return rows.TryGetValue(key, out value); }
        public IEnumerator<KeyValuePair<uint, EngineRow<uint>>> GetEnumerator() => throw new InvalidOperationException("Unexpected scan.");
        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    }
    [Theory]
    [InlineData(8)]
    [InlineData(512)]
    public void OneChangedRowUsesBoundedPointLookupsAndReusesUnchangedRoots(int count)
    {
        var rows = Enumerable.Range(1, count).ToDictionary(n => (uint)n, n => new EngineRow<uint>(new((uint)n, (uint)n, (uint)n), $"name {n}"));
        var index = ConstraintIndex<uint>.Build(Options(Enum.GetValues<EngineConstraint>()), rows)!;
        var changed = new Dictionary<uint, EngineRow<uint>>(rows) { [1] = rows[1] with { Name = "changed" } };
        var before = new PointRows(rows); var after = new PointRows(changed);
        var candidate = index.Prepare(before, after, new[] { 1U });
        Assert.InRange(before.Lookups + after.Lookups, 1, 7);
        Assert.Same(index.Pairs, candidate.Pairs); Assert.Same(index.References, candidate.References); Assert.NotSame(index.Names, candidate.Names);
        Assert.Equal(1U, index.Names!["name 1"]); Assert.False(index.Names.ContainsKey("changed")); Assert.Equal(1U, candidate.Names!["changed"]);
        var noop = index.Prepare(rows, rows, new[] { 1U });
        Assert.Same(index.Pairs, noop.Pairs); Assert.Same(index.Names, noop.Names); Assert.Same(index.References, noop.References);
    }

    private sealed class FaultLog : ICommitLog
    {
        private readonly MemoryCommitLog _inner = new();
        internal string Mode = "";
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => _inner.Read();
        public void Append(string record)
        {
            if (Mode == "append-before") throw new IOException("Before append.");
            _inner.Append(record);
            if (Mode == "append-after") throw new IOException("After append.");
        }
        public void Flush()
        {
            if (Mode == "flush") throw new IOException("Unknown flush.");
            _inner.Flush();
        }
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void UnknownIoKeepsCommittedRootsAndRebuildsTheActualRecoveredPrefix(bool group)
    {
        foreach (var mode in new[] { "append-before", "append-after", "flush" })
        {
            var log = new FaultLog(); var options = Options(Enum.GetValues<EngineConstraint>()) with { StateStructure = StateStructure.Shared, RevisionEncoding = RevisionEncoding.Changes, Isolation = IsolationLevel.Snapshot };
            var engine = new LinksEngine<uint>(log, options);
            using (var seed = engine.Begin())
            { seed.CreateLink(1, 1); seed.SetName(1, "one"); seed.CreateLink(2, 2); seed.SetName(2, "two"); seed.Commit(); }
            var rows = engine.Current; var index = engine.ConstraintIndex!; log.Mode = mode;
            using var a = engine.Begin(); a.SetName(1, "new one");
            if (group)
            {
                using var b = engine.Begin(); b.SetName(2, "new two");
                Assert.All(engine.CommitGroup(new[] { a, b }), outcome => Assert.Equal(GroupCommitStatus.Indeterminate, outcome.Status));
            }
            else Assert.Throws<IOException>(() => a.Commit());
            Assert.Same(rows, engine.Current); Assert.Same(index, engine.ConstraintIndex);
            Assert.Equal(1U, index.Names!["one"]); Assert.False(index.Names.ContainsKey("new one"));
            log.Mode = ""; var reopened = new LinksEngine<uint>(log, options);
            using (var bad = reopened.Begin())
            { bad.SetName(2, reopened.Snapshot().Rows[1].Name!); Assert.Throws<InvalidOperationException>(() => bad.Commit()); }
            using var following = reopened.Begin(); following.SetName(2, "following"); following.Commit();
        }
    }
}
