using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CausalSessionTests
{
    private const string Alice = "00000000000000000000000000000001", Bob = "00000000000000000000000000000002";
    private static string Token(int value) => value.ToString("x32", CultureInfo.InvariantCulture);
    private sealed class Store : IAtomicJournalStore
    {
        public bool SupportsDurableFlush => true;
        internal readonly Dictionary<string, JournalVersion> Values = new(StringComparer.Ordinal);
        internal int Calls, Failure;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => keys.Select(key =>
            Values.TryGetValue(key, out var value) ? value with { Entries = value.Entries.ToArray() } : new JournalVersion(0, [])).ToArray();
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            Calls++; int failure = Failure; Failure = 0;
            if (failure == 1) throw new IOException("Before publication.");
            if (keys.Where((key, i) => Values.GetValueOrDefault(key)?.Generation != expected[i].Generation &&
                !(Values.GetValueOrDefault(key) is null && expected[i].Generation == 0)).Any()) return false;
            for (int i = 0; i < keys.Count; i++) Values[keys[i]] = new(expected[i].Generation + 1, updates[i]!.ToArray());
            if (failure == 2) throw new IOException("Lost publication reply.");
            return true;
        }
    }
    private static CausalBatch Batch(Store store) => new(store, "replica", "batches", Alice);
    private static CausalReplica Owner(Store store) => new(store, "replica", Alice);
    private static Store Seeded()
    {
        var store = new Store();
        Batch(store).Write(Token(10), 0, [new(1, [], new(0, 2, "old")), new(3, [], new(1, 1, null)), new(4, [], new(0, 2, null))]);
        return store;
    }
    private static CausalSession Session(Store store, ulong first = 32, ulong last = 33) =>
        new(Batch(store), first, last, CausalConflictPolicy.MaxDot);
    private static string Bytes(Store store) => JsonSerializer.Serialize(store.Values.OrderBy(v => v.Key, StringComparer.Ordinal));
    private static string[] Wires(IEnumerable<CausalVersion> values) => values.Select(v => v.Encode()).ToArray();

    [Fact]
    public void OrdinaryLinksNamesAndObserversPublishOneOwnedBatch()
    {
        var store = Seeded(); string before = Bytes(store); int calls = store.Calls;
        using var tx = Session(store); Assert.Equal("old", tx.GetName(1));
        int observed = 0; ulong created = 0;
        tx.Create(null, (old, row) => { Assert.Null(old); created = row![0]; observed++; return tx.Constants.Continue; });
        Assert.Equal(1, observed); Assert.Equal(32UL, created);
        tx.UpdateLink(32, 0, 2); tx.SetName(32, "\ufeff雪😀\0");
        tx.UpdateLink(1, 2, 0); tx.DeleteLink(1);
        Assert.NotNull(tx.GetLink(3)); Assert.NotNull(tx.GetLink(4));
        Assert.Equal(32UL, tx.GetByName("\ufeff雪😀\0"));
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
        var result = tx.Commit(Token(100)); Assert.Equal(new[] { 1UL, 32UL }, result.Select(v => v.Address));
        Assert.Null(result[0].Payload); Assert.Equal(calls + 1, store.Calls);
        result[1] = result[1] with { Payload = new(0, 0, "changed returned result") };
        Assert.Equal("\ufeff雪😀\0", tx.Commit(Token(100))[1].Payload!.Name);
        Assert.ThrowsAny<Exception>(() => tx.Commit(Token(200)));
        Assert.ThrowsAny<Exception>(() => tx.CreateLink(0, 0));
        Assert.Equal(5UL, Batch(store).Snapshot().LastCounter);
    }

    [Fact]
    public void ReservationCursorIsIndependentOfPrivateJournalAllocator()
    {
        var store = new Store(); Batch(store).Write(Token(10), 0, [new(ulong.MaxValue, [], new(0, 0, null))]);
        using var tx = Session(store); Assert.Equal(32UL, tx.CreateLink(0, 0));
        Assert.ThrowsAny<Exception>(() => tx.EnsureCreated(31)); Assert.ThrowsAny<Exception>(() => tx.EnsureCreated(34));
        Assert.Equal(33UL, tx.EnsureCreated(33)); var before = tx.Rows();
        Assert.ThrowsAny<Exception>(() => tx.CreateLink(0, 0)); Assert.Equal(before, tx.Rows());
        tx.DeleteLink(ulong.MaxValue); tx.EnsureCreated(ulong.MaxValue); tx.SetName(ulong.MaxValue, "known identity");
        Assert.Equal(3, tx.Commit(Token(100)).Length);
        using var last = Session(new Store(), ulong.MaxValue, ulong.MaxValue);
        Assert.Equal(ulong.MaxValue, last.CreateLink(0, 0)); Assert.ThrowsAny<Exception>(() => last.CreateLink(0, 0));
        Assert.Single(last.Commit(Token(100)));
    }

    [Fact]
    public void CancellationEmptyReceiptAndRepeatedCleanupConsumeNoAuthorCounter()
    {
        var store = Seeded(); using var tx = Session(store);
        tx.SetName(1, "temporary"); tx.SetName(1, "old"); var id = tx.CreateLink(0, 0); tx.DeleteLink(id);
        int calls = store.Calls; Assert.Empty(tx.Commit(Token(100)));
        Assert.Equal(3UL, Batch(store).Snapshot().LastCounter); Assert.Equal(calls + 1, store.Calls);
        Assert.Empty(tx.Commit(Token(100))); Assert.Equal(calls + 1, store.Calls);
        using var abandoned = Session(store, 40, 41); abandoned.DeleteLink(1); abandoned.Rollback(); abandoned.Rollback();
        Assert.ThrowsAny<Exception>(() => abandoned.Commit(Token(200))); Assert.Equal(calls + 1, store.Calls);
    }

    [Fact]
    public void FrozenUnknownRequestsRetryExactlyWithoutRebaseAfterFollowingWrite()
    {
        foreach (int failure in new[] { 1, 2 })
        {
            var store = Seeded(); using var tx = Session(store); tx.SetName(1, "new"); store.Failure = failure;
            Assert.Contains("Indeterminate", Assert.Throws<IOException>(() => tx.Commit(Token(100))).Message);
            var request = tx.Prepared!; Assert.Equal(Token(100), request.Token); Assert.Single(request.Edits); Assert.Single(request.Edits[0].Observed);
            Assert.ThrowsAny<Exception>(() => tx.SetName(1, "changed")); Assert.ThrowsAny<Exception>(() => tx.Commit(Token(200)));
            if (failure == 2) Owner(store).Write(Token(300), 9, [], null);
            var result = tx.Commit(Token(100)); Assert.Equal(4UL, result[0].Dot.Counter); string before = Bytes(store);
            Assert.Equal(Wires(result), Wires(tx.Commit(Token(100)))); Assert.Equal(before, Bytes(store));
        }
    }

    [Fact]
    public void LosingDeletionsRemainObservedAndConstraintsValidateBeforePublication()
    {
        var store = Seeded(); Owner(store).Receive([CausalVersion.Advance(1, Bob, 0, [], null)]);
        using var tx = Session(store); Assert.Null(tx.GetLink(1)); tx.EnsureCreated(1); tx.SetName(1, "recreated");
        Assert.Equal(2, tx.Prepare(Token(100)).Edits[0].Observed.Count); Assert.Single(tx.Commit(Token(100)));
        Assert.Single(Batch(store).Snapshot().Versions, v => v.Address == 1);
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var validation in Enum.GetValues<ConstraintValidation>())
            {
                var constrained = new Store(); using var invalid = new CausalSession(Batch(constrained), 32, 33, CausalConflictPolicy.Reject,
                    new() { StateStructure = structure, ConstraintValidation = validation, Constraints = [EngineConstraint.UniquePairs] });
                invalid.CreateLink(0, 0); invalid.CreateLink(0, 0);
                Assert.ThrowsAny<Exception>(() => invalid.Commit(Token(100))); Assert.Empty(constrained.Values); Assert.Equal(0, constrained.Calls);
            }
    }

    [Fact]
    public void StaleReservationsAndBoundedFinalBatchesNeverPublishPartialEffects()
    {
        var store = Seeded(); using var stale = Session(store); stale.DeleteLink(1); Owner(store).Write(Token(300), 9, [], null);
        string before = Bytes(store); int calls = store.Calls;
        Assert.ThrowsAny<Exception>(() => stale.Commit(Token(100))); Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
        foreach (var (first, last) in new (ulong, ulong)[] { (0, 1), (33, 32), (1, 1) })
            Assert.ThrowsAny<Exception>(() => Session(store, first, last));
        var empty = new Store(); using var full = Session(empty, 1, 65);
        for (int i = 0; i < 65; i++) full.CreateLink(0, 0);
        Assert.ThrowsAny<Exception>(() => full.Commit(Token(100))); Assert.Empty(empty.Values); Assert.Equal(0, empty.Calls);
    }

    [Fact]
    public void PreparedRequestsAndFinishedRowsAreOwnedAcrossRepeatedCleanup()
    {
        var store = Seeded(); using var tx = Session(store); tx.SetName(1, ""); var request = tx.Prepare(Token(100));
        Assert.Equal("", request.Edits[0].Payload!.Name);
        ((CausalBatchEdit[])request.Edits)[0] = new(1, [], new(0, 0, "caller mutation"));
        var rows = tx.Rows(); Assert.Equal("", rows[0].Name); rows[0] = rows[0] with { Name = null };
        Assert.Equal("", tx.Commit(Token(100))[0].Payload!.Name); Assert.Equal("", tx.Rows()[0].Name);
        int calls = store.Calls; var retained = tx.Rows(); tx.Dispose(); tx.Dispose();
        Assert.Equal("", retained[0].Name); Assert.Empty(tx.Rows());
        Assert.ThrowsAny<Exception>(() => tx.Commit(Token(100))); Assert.Equal(calls, store.Calls); Assert.Equal(4UL, Batch(store).Snapshot().LastCounter);
    }

    [Fact]
    public void TokenAndPayloadPreflightRejectWithoutPublication()
    {
        foreach (string invalid in new[] { "", "invalid", new string('0', 32), new string('F', 32) })
        {
            var store = Seeded(); string before = Bytes(store); int calls = store.Calls; using var tx = Session(store); tx.SetName(1, "new");
            Assert.ThrowsAny<Exception>(() => tx.Prepare(invalid)); Assert.Null(tx.Prepared);
            Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
            tx.SetName(1, "valid after rejection"); Assert.Single(tx.Commit(Token(100)));
        }
        var bounded = Seeded(); string bytes = Bytes(bounded); int count = bounded.Calls;
        using var tooLong = Session(bounded); tooLong.SetName(1, new string('a', 4097));
        Assert.ThrowsAny<Exception>(() => tooLong.Prepare(Token(100))); Assert.Null(tooLong.Prepared);
        Assert.Equal(bytes, Bytes(bounded)); Assert.Equal(count, bounded.Calls);
    }
    [Fact]
    public void StandardCallbacksRestrictionsAndFrozenReadsKeepOwnedValues()
    {
        var store = Seeded(); using var tx = Session(store); ulong any = tx.Constants.Any;
        Assert.Equal(3UL, tx.Count(null)); Assert.Equal(3UL, tx.Count([]));
        Assert.Equal(1UL, tx.Count([1])); Assert.Equal(2UL, tx.Count([any, 2]));
        Assert.Equal(1UL, tx.Count([1, 2])); Assert.Equal(0UL, tx.Count([3, 2]));
        Assert.Equal(0UL, tx.Count([9, any])); Assert.Equal(3UL, tx.Count([any, any]));
        Assert.Equal(1UL, tx.Count([any, 1])); Assert.Equal(0UL, tx.Count([any, 9]));
        Assert.Equal(2UL, tx.Count([any, 0, 2])); Assert.Equal(1UL, tx.Count([3, 1, 1]));
        Assert.Equal(0UL, tx.Count([9, any, any]));
        Assert.Throws<ArgumentException>(() => tx.Count([any, any, any, any]));
        Assert.Equal(tx.Constants.Continue, tx.Each(null, null));
        int callbacks = 0;
        Assert.Equal(tx.Constants.Break, tx.Each(null, row => { callbacks++; return tx.Constants.Break; })); Assert.Equal(1, callbacks);
        Assert.Equal(tx.Constants.Continue, tx.Each(null, row => { callbacks++; return tx.Constants.Continue; })); Assert.Equal(4, callbacks);
        Assert.Equal(tx.Constants.Continue, tx.Create(null, null)); Assert.NotNull(tx.GetLink(32));
        Assert.Equal(tx.Constants.Break, tx.Update([32], [32, 1, 2], (before, after) =>
        { Assert.Equal(new[] { 32UL, 0UL, 0UL }, before); Assert.Equal(new[] { 32UL, 1UL, 2UL }, after); return tx.Constants.Break; }));
        Assert.Equal(tx.Constants.Continue, tx.Update([32], [32, 2, 1], null));
        Assert.Equal(tx.Constants.Break, tx.Delete([32], (before, after) =>
        { Assert.Equal(new[] { 32UL, 2UL, 1UL }, before); Assert.Null(after); return tx.Constants.Break; }));
        Assert.Equal(tx.Constants.Continue, tx.Delete([4], null));
        Assert.Throws<ArgumentException>(() => tx.Update(null, [1, 2, 3], null));
        Assert.Throws<ArgumentException>(() => tx.Update([1], null, null));
        Assert.Throws<ArgumentException>(() => tx.Delete(null, null));
        var unchanged = tx.Rows(); Assert.ThrowsAny<Exception>(() => tx.EnsureCreated(0)); Assert.Equal(unchanged, tx.Rows());
        Assert.Equal(0UL, tx.GetByName("missing")); Assert.Null(tx.GetName(9));
        tx.SetName(1, "renamed"); tx.RemoveName(1); tx.SetName(1, "final");
        tx.Prepare(Token(100));
        Assert.Equal("final", tx.GetName(1)); Assert.Null(tx.GetName(9));
        Assert.Equal(1UL, tx.GetByName("final")); Assert.Equal(0UL, tx.GetByName("missing"));
        Assert.NotNull(tx.GetLink(1)); Assert.Null(tx.GetLink(9)); Assert.Equal(2UL, tx.Count(null));
        Assert.Equal(2, tx.Commit(Token(100)).Length);
        tx.Dispose(); Assert.Null(tx.GetLink(1)); Assert.Null(tx.GetName(1)); Assert.Equal(0UL, tx.GetByName("final"));
        Assert.ThrowsAny<Exception>(() => tx.RemoveName(1)); Assert.ThrowsAny<Exception>(() => tx.EnsureCreated(1));
        string bytes = Bytes(store);
        Assert.ThrowsAny<Exception>(() => new CausalSession(Batch(store), 40, 41, CausalConflictPolicy.Reject,
            new() { Constraints = [EngineConstraint.References] }));
        Assert.Equal(bytes, Bytes(store));
        var empty = new Store(); using var overflow = Session(empty, 1, 2); overflow.CreateLink(0, 0); overflow.CreateLink(0, 0);
        Assert.ThrowsAny<Exception>(() => overflow.Prepare(new string('f', 32))); Assert.Empty(empty.Values); Assert.Equal(0, empty.Calls);
    }

    [Fact]
    public void IndependentFiniteSchedulesMatchRowsDifferencesAndExactRetries()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "causal-session-v1.json")));
        Assert.Equal(343, vectors.RootElement.GetProperty("cases").GetArrayLength());
        foreach (var item in vectors.RootElement.GetProperty("cases").EnumerateArray())
            foreach (int failure in new[] { 0, 1, 2 })
            {
                var store = Seeded(); string initial = Bytes(store); int calls = store.Calls; using var tx = Session(store);
                int position = 0;
                foreach (var operation in item.GetProperty("operations").EnumerateArray())
                {
                    var before = tx.Rows();
                    Exception? error = Record.Exception(() =>
                    {
                        switch (operation.GetString())
                        {
                            case "create": tx.CreateLink(0, 2); break;
                            case "rename": tx.SetName(1, "new\ufeff\0🙂"); break;
                            case "update": tx.UpdateLink(1, 2, 0); break;
                            case "delete": tx.DeleteLink(1); break;
                            case "restore": tx.EnsureCreated(1); tx.UpdateLink(1, 0, 2); tx.SetName(1, "old"); break;
                            case "cancel-create": if (tx.GetLink(32) is not null) tx.DeleteLink(32); break;
                            case "noop": break;
                            default: throw new InvalidOperationException("Unknown reference operation.");
                        }
                    });
                    Assert.Equal(item.GetProperty("errors")[position++].GetBoolean(), error is not null);
                    if (error is not null) Assert.Equal(before, tx.Rows());
                    Assert.Equal(initial, Bytes(store)); Assert.Equal(calls, store.Calls);
                }
                string rows = JsonSerializer.Serialize(tx.Rows().Select(row => new object?[]
                { row.Link.Index.ToString(CultureInfo.InvariantCulture), row.Link.Source.ToString(CultureInfo.InvariantCulture), row.Link.Target.ToString(CultureInfo.InvariantCulture), row.Name }));
                Assert.Equal(JsonSerializer.Serialize(item.GetProperty("rows")), rows);
                store.Failure = failure; var first = Record.Exception(() => tx.Commit(Token(100)));
                Assert.Equal(failure != 0, first is not null);
                var written = tx.Commit(Token(100));
                Assert.Equal(item.GetProperty("changed").EnumerateArray().Select(v => v.GetString()), written.Select(v => v.Address.ToString(CultureInfo.InvariantCulture)));
                Assert.Equal(3UL + (ulong)written.Length, Batch(store).Snapshot().LastCounter);
                Assert.Equal(Wires(written), Wires(tx.Commit(Token(100))));
                Assert.Equal(calls + (failure == 1 ? 2 : 1), store.Calls);
            }
    }

}
