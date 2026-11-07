using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CausalBatchTests
{
    private const string Alice = "00000000000000000000000000000001", Bob = "00000000000000000000000000000002";
    private static string Token(int value) => value.ToString("x32", CultureInfo.InvariantCulture);
    private static string[] Wires(IEnumerable<CausalVersion> values) => values.Select(v => v.Encode()).ToArray();
    private sealed class Store : IAtomicJournalStore
    {
        public bool SupportsDurableFlush { get; set; } = true;
        internal readonly Dictionary<string, JournalVersion> Values = new(StringComparer.Ordinal);
        internal int Calls, Failure;
        internal bool NullRead;
        internal Action? Before;
        internal IReadOnlyList<JournalVersion>? Corrupt;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => NullRead ? null! : Corrupt ??
            keys.Select(key => Values.TryGetValue(key, out var value) ? value with { Entries = value.Entries.ToArray() } : new JournalVersion(0, [])).ToArray();
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            var before = Before; Before = null; before?.Invoke(); Calls++;
            int failure = Failure; Failure = 0;
            if (failure == 1) throw new IOException("Before publication.");
            if (keys.Where((key, index) => Values.GetValueOrDefault(key)?.Generation != expected[index].Generation &&
                !(Values.GetValueOrDefault(key) is null && expected[index].Generation == 0)).Any()) return false;
            for (int index = 0; index < keys.Count; index++) Values[keys[index]] = new(expected[index].Generation + 1, updates[index]!.ToArray());
            if (failure == 2) throw new IOException("Lost publication reply.");
            return true;
        }
    }
    private static CausalBatch Batch(Store store, CausalReplicaOptions? options = null) => new(store, "replica", "batches", Alice, options);
    private static CausalReplica Owner(Store store) => new(store, "replica", Alice);
    private static CausalBatchEdit[] Edits() => [new(1, [], new(0, 2, "\ufeff雪😀\0")), new(2, [], null)];
    private static string Bytes(Store store) => JsonSerializer.Serialize(store.Values.OrderBy(v => v.Key, StringComparer.Ordinal));

    [Fact]
    public void IndependentBatchJournalVectorsRejectWithoutPublication()
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "causal-batch-v1.json")));
        foreach (var fixture in document.RootElement.GetProperty("cases").EnumerateArray())
        {
            var store = new Store();
            foreach (var (key, field) in new[] { ("replica", "owner"), ("batches", "receipts") })
            {
                var value = fixture.GetProperty(field);
                store.Values[key] = new(ulong.Parse(value.GetProperty("generation").GetString()!, CultureInfo.InvariantCulture),
                    value.GetProperty("entries").EnumerateArray().Select(v => v.GetString()!).ToArray());
            }
            var options = fixture.GetProperty("options");
            var batch = Batch(store, new(options.TryGetProperty("maxEntries", out var entries) ? entries.GetInt32() : 4096,
                options.TryGetProperty("maxBytes", out var bytes) ? bytes.GetInt32() : 8_388_608));
            string before = Bytes(store);
            if (fixture.GetProperty("valid").GetBoolean())
            {
                var result = batch.Snapshot();
                Assert.Equal(fixture.GetProperty("owner").GetProperty("generation").GetString(), result.Generation.ToString(CultureInfo.InvariantCulture));
                Assert.Equal(fixture.GetProperty("lastCounter").GetString(), result.LastCounter.ToString(CultureInfo.InvariantCulture));
                Assert.Equal(fixture.GetProperty("versions").EnumerateArray().Select(v => JsonSerializer.Serialize(v)), Wires(result.Versions));
            }
            else Assert.ThrowsAny<Exception>(() => batch.Snapshot());
            Assert.Equal(before, Bytes(store)); Assert.Equal(0, store.Calls);
        }
    }

    [Fact]
    public void WholeBatchAndExactRetrySurviveFollowingWritesAndLostReplies()
    {
        foreach (int failure in new[] { 0, 1, 2 })
        {
            var store = new Store { Failure = failure };
            if (failure == 0) Batch(store).Write(Token(100), 0, Edits());
            else Assert.Contains("Indeterminate", Assert.Throws<IOException>(() => Batch(store).Write(Token(100), 0, Edits())).Message);
            Assert.Equal(1, store.Calls); if (failure == 1) Assert.Empty(store.Values);
            var first = Batch(store).Write(Token(100), 0, Edits());
            Assert.Equal(new[] { 1UL, 2UL }, first.Select(v => v.Dot.Counter));
            Assert.Equal(3UL, Owner(store).Write(Token(300), 3, [], null).Dot.Counter);
            string before = Bytes(store); int calls = store.Calls;
            Assert.Equal(Wires(first), Wires(Batch(store).Write(Token(100), 0, Edits())));
            var changed = Edits(); changed[1] = changed[1] with { Payload = changed[0].Payload };
            Assert.Throws<InvalidDataException>(() => Batch(store).Write(Token(100), 0, changed));
            Assert.Throws<InvalidDataException>(() => Batch(store).Write(Token(100), 2, Edits()));
            Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
            ((CausalDot[])first[0].Clock)[0] = new(Alice, 99);
            Assert.Equal(1UL, Batch(store).Write(Token(100), 0, Edits())[0].Dot.Counter);
            Assert.Empty(Batch(store).Write(Token(400), Batch(store).Snapshot().Generation, []));
            Assert.Equal(3UL, Batch(store).Snapshot().LastCounter);
        }
    }

    [Fact]
    public void LaterContradictionChildTokenCollisionAndStaleBaseHaveNoEffects()
    {
        var store = new Store(); var remote = CausalVersion.Advance(2, Bob, 0, [], null); Owner(store).Receive([remote]);
        var edits = Edits(); edits[1] = edits[1] with { Observed = [remote with { Payload = edits[0].Payload }] };
        string before = Bytes(store); int calls = store.Calls; ulong generation = Batch(store).Snapshot().Generation;
        Assert.Throws<InvalidDataException>(() => Batch(store).Write(Token(100), generation, edits));
        Assert.Throws<InvalidOperationException>(() => Batch(store).Write(Token(100), 0, Edits()));
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
        Owner(store).Write(Token(100), 1, [], Edits()[0].Payload);
        generation = Batch(store).Snapshot().Generation; before = Bytes(store); calls = store.Calls;
        Assert.Throws<InvalidDataException>(() => Batch(store).Write(Token(100), generation, Edits()));
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
    }

    [Fact]
    public void ConflictingPublicationAndFiniteAdmissionPreserveStoredBytes()
    {
        var store = new Store(); store.Before = () => Batch(store).Write(Token(200), 0, Edits());
        Assert.Throws<InvalidOperationException>(() => Batch(store).Write(Token(100), 0, Edits()));
        Assert.Equal(2UL, Batch(store).Snapshot().LastCounter);
        string before = Bytes(store); int calls = store.Calls;
        foreach (var edits in new[] { Enumerable.Repeat(Edits()[0], 65).ToArray(), new[] { Edits()[0], Edits()[0] } })
            Assert.Throws<InvalidDataException>(() => Batch(store).Write(Token(300), 1, edits));
        Assert.Throws<InvalidDataException>(() => Batch(store).Write(new('f', 32), 1, Edits()));
        foreach (string token in new[] { null!, "invalid" })
            Assert.Throws<InvalidDataException>(() => Batch(store).Write(token, 1, []));
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
        foreach (string key in new[] { "", "replica", "bad/key" }) Assert.Throws<InvalidDataException>(() => new CausalBatch(store, "replica", key, Alice));
        store.Corrupt = []; Assert.Throws<InvalidDataException>(() => Batch(store).Snapshot());
    }

    [Fact]
    public void MaximumEditsEmptyPublicationAndCapacityExhaustion()
    {
        var store = new Store(); var request = Enumerable.Range(1, 64).Select(i => new CausalBatchEdit((ulong)i, [], null)).ToArray();
        var result = Batch(store).Write(Token(100), 0, request);
        Assert.Equal(64, result.Length); Assert.Equal(64UL, result[^1].Dot.Counter); Assert.Equal(1, store.Calls);
        var limited = new Store(); var batch = Batch(limited, new(MaxEntries: 3));
        batch.Write(Token(100), 0, Edits()); batch.Write(Token(200), 1, []);
        string before = Bytes(limited); int calls = limited.Calls;
        Assert.Throws<InvalidDataException>(() => batch.Write(Token(300), 2, []));
        Assert.Equal(before, Bytes(limited)); Assert.Equal(calls, limited.Calls);
        var empty = new Store(); Assert.Empty(Batch(empty).Write(Token(100), 0, []));
        before = Bytes(empty); calls = empty.Calls;
        Assert.Empty(Batch(empty).Write(Token(100), 0, []));
        Assert.Equal(before, Bytes(empty)); Assert.Equal(calls, empty.Calls); Assert.Equal(0UL, Batch(empty).Snapshot().LastCounter);
        var small = new Store(); Assert.Throws<InvalidDataException>(() => Batch(small, new(MaxEntries: 2)).Write(Token(100), 0, Edits()));
        Assert.Empty(small.Values); Assert.Equal(0, small.Calls);
    }

    [Fact]
    public void MatchingRetryStillRejectsDominatedHistoricalEquivocation()
    {
        var store = new Store(); var old = CausalVersion.Advance(1, Bob, 0, [], Edits()[0].Payload);
        var newer = CausalVersion.Advance(1, Bob, 1, [old], null);
        var first = Batch(store).Write(Token(100), 0, [new(1, [old, newer], null)]);
        Assert.Equal(Wires(first), Wires(Batch(store).Write(Token(100), 0, [new(1, [newer], null)])));
        string before = Bytes(store); int calls = store.Calls;
        Assert.Throws<InvalidDataException>(() => Batch(store).Write(Token(100), 0, [new(1, [old with { Payload = null }, newer], null)]));
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
    }

    [Fact]
    public void ProviderFramesDurabilityAndCallerAdmissionRejectBeforePublication()
    {
        var store = new Store(); var batch = Batch(store, new(RequireDurableFlush: true));
        Assert.Empty(batch.Snapshot().Versions);
        Assert.Empty(batch.Write(Token(100), 0, []));
        store.SupportsDurableFlush = false;
        Assert.Throws<InvalidDataException>(() => Batch(store, new(RequireDurableFlush: true)));
        string before = Bytes(store); int calls = store.Calls;
        foreach (string key in new[] { new string('a', 129), ".valid_-09Z/" })
            Assert.Throws<InvalidDataException>(() => new CausalBatch(store, "replica", key, Alice));
        _ = new CausalBatch(store, "replica", ".valid_-09Z", Alice);
        Assert.Throws<InvalidDataException>(() => batch.Write(Token(200), 1, null!));
        foreach (var request in new CausalBatchEdit[][] { [null!], [new(1, null!, null)] })
            Assert.Throws<InvalidDataException>(() => batch.Write(Token(200), 1, request));
        foreach (var result in new IReadOnlyList<JournalVersion>[]
        {
            [null!, new(0, [])], [new(0, null!), new(0, [])],
            [new(0, []), new(1, [null!])],
        })
        {
            store.Corrupt = result;
            Assert.Throws<InvalidDataException>(() => batch.Snapshot());
        }
        store.Corrupt = null;
        store.NullRead = true;
        Assert.Throws<InvalidDataException>(() => batch.Snapshot());
        store.NullRead = false;
        string header = JsonSerializer.Serialize(new[] { "causal-batches1", "replica", Alice });
        foreach (string frame in new[] { "]", "[", "\"", "\\", "\x1f", "\x7f", "}",
            "[" + string.Join(',', Enumerable.Repeat("null", 32770)) + "]", new string('1', 1_048_577) })
        {
            store.Corrupt = [store.Values["replica"], new(1, [header, frame])];
            Assert.Throws<InvalidDataException>(() => batch.Snapshot());
        }
        store.Corrupt = null;
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
    }
}
