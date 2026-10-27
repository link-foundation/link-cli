using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CausalReplicaTests
{
    private const string Alice = "00000000000000000000000000000001", Bob = "00000000000000000000000000000002";
    private static string Token(int value) => value.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalPayload Row(string name) => new(2, 3, name);
    private static string[] Wires(IEnumerable<CausalVersion> values) => values.Select(v => v.Encode()).ToArray();
    private sealed class Store : IAtomicJournalStore
    {
        public bool SupportsDurableFlush { get; set; } = true;
        internal ulong Generation;
        internal string[] Entries = [];
        internal int Calls, Failure;
        internal Action? BeforeCompare;
        internal IReadOnlyList<JournalVersion>? Corrupt;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        { Assert.Equal(new[] { "replica" }, keys); return Corrupt ?? [new(Generation, Entries.ToArray())]; }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected,
            IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            Assert.Equal(new[] { "replica" }, keys); Calls++;
            int failure = Failure; Failure = 0;
            if (failure == 1) throw new IOException("Before publication.");
            var before = BeforeCompare; BeforeCompare = null; before?.Invoke();
            if (Generation != expected[0].Generation) return false;
            Generation++; Entries = updates[0]!.ToArray();
            if (failure == 2) throw new IOException("Lost reply."); return true;
        }
    }
    private static CausalReplica Owner(Store store, string author = Alice, CausalReplicaOptions? options = null) =>
        new(store, "replica", author, options);

    [Fact]
    public void IndependentJournalVectorsValidateWithoutPublishing()
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory,
            "Protocol", "causal-replica-v1.json")));
        foreach (var fixture in document.RootElement.GetProperty("cases").EnumerateArray())
        {
            var store = new Store
            {
                Generation = ulong.Parse(fixture.GetProperty("generation").GetString()!, CultureInfo.InvariantCulture),
                Entries = fixture.GetProperty("entries").EnumerateArray().Select(v => v.GetString()!).ToArray()
            };
            var options = fixture.GetProperty("options");
            var owner = Owner(store, options: new(
                options.TryGetProperty("maxEntries", out var entries) ? entries.GetInt32() : 4096,
                options.TryGetProperty("maxBytes", out var bytes) ? bytes.GetInt32() : 8_388_608));
            var before = store.Entries.ToArray();
            if (fixture.GetProperty("valid").GetBoolean())
            {
                var result = owner.Snapshot();
                Assert.Equal(fixture.GetProperty("lastCounter").GetString(), result.LastCounter.ToString(CultureInfo.InvariantCulture));
                Assert.Equal(fixture.GetProperty("versions").EnumerateArray().Select(v => JsonSerializer.Serialize(v)), Wires(result.Versions));
            }
            else Assert.ThrowsAny<Exception>(() => owner.Snapshot());
            Assert.Equal(0, store.Calls); Assert.Equal(before, store.Entries);
        }
    }

    [Fact]
    public void RetryReceiptsSurviveFollowingEditsAndReopeningWithOwnedResults()
    {
        var store = new Store(); var first = Owner(store);
        var initial = first.Write(Token(1), 1, [], Row("initial"));
        var edited = first.Write(Token(2), 1, [initial], Row("edited"));
        var reopened = Owner(store); int calls = store.Calls;
        Assert.Equal(initial.Encode(), reopened.Write(Token(1), 1, [], Row("initial")).Encode());
        Assert.Equal(calls, store.Calls); Assert.Equal(Wires([edited]), Wires(reopened.Read(1)));
        Assert.Throws<InvalidDataException>(() => reopened.Write(Token(1), 1, [], Row("changed retry")));
        Assert.Throws<InvalidDataException>(() => reopened.Write(Token(1), 1, [initial], Row("initial")));
        ((CausalDot[])initial.Clock)[0] = new(Alice, 99);
        Assert.Equal(2UL, reopened.Snapshot().LastCounter);
        Assert.Equal(2UL, reopened.Read(1).Single().Clock.Single().Counter);
    }

    [Fact]
    public void ActiveHandlesFenceCounterPublicationAtLocalCas()
    {
        var store = new Store(); var left = Owner(store); var right = Owner(store);
        store.BeforeCompare = () => right.Write(Token(2), 2, [], Row("right"));
        Assert.Throws<InvalidOperationException>(() => left.Write(Token(1), 1, [], Row("left")));
        Assert.Equal(2, store.Calls); Assert.Equal(1UL, left.Snapshot().LastCounter);
        Assert.Equal(2UL, left.Write(Token(1), 1, [], Row("left")).Dot.Counter);
        Assert.Equal(new[] { 1UL, 2UL }, left.Snapshot().Versions.Select(v => v.Dot.Counter).Order());
    }

    [Fact]
    public void BeforeAndAfterPublicationFailuresRequireExplicitTokenRetry()
    {
        foreach (int failure in new[] { 1, 2 })
        {
            var store = new Store { Failure = failure }; var first = Owner(store);
            Assert.Contains("Indeterminate", Assert.Throws<IOException>(() => first.Write(Token(1), 1, [], Row("uncertain"))).Message);
            Assert.Equal(1, store.Calls);
            var following = first.Write(Token(2), 2, [], Row("following")); int calls = store.Calls;
            var reopened = Owner(store); var retry = reopened.Write(Token(1), 1, [], Row("uncertain"));
            Assert.Equal(failure == 1 ? (1UL, 2UL) : (2UL, 1UL), (following.Dot.Counter, retry.Dot.Counter));
            Assert.Equal(calls + (failure == 1 ? 1 : 0), store.Calls); Assert.Equal(2UL, reopened.Snapshot().LastCounter);
        }
    }

    [Fact]
    public void IndependentWritersRetainConcurrentDeletionAndRejectResurrection()
    {
        var left = Owner(new()); var right = Owner(new(), Bob);
        var initial = left.Write(Token(1), 1, [], Row("initial")); Assert.True(right.Receive([initial]));
        var update = left.Write(Token(2), 1, [initial], Row("update"));
        var deletion = right.Write(Token(1), 1, [initial], null);
        left.Receive([deletion]); right.Receive([update]);
        Assert.Equal(Wires(left.Read(1)), Wires(right.Read(1))); Assert.Equal(2, left.Read(1).Length);
        var resolved = left.Write(Token(3), 1, left.Read(1), null); right.Receive([resolved]);
        Assert.False(right.Receive([initial])); Assert.Equal(Wires([resolved]), Wires(right.Read(1)));
        Assert.Equal(1UL, right.Snapshot().LastCounter); Assert.Empty(right.Read(2));
    }

    [Fact]
    public void HistoricalContradictionsAndForeignLocalCountersRejectBeforeCas()
    {
        var store = new Store(); var owner = Owner(store);
        var remote = CausalVersion.Advance(1, Bob, 0, [], Row("remote")); owner.Receive([remote]);
        owner.Write(Token(1), 1, [remote], Row("resolved"));
        var resolved = owner.Read(1).Single();
        var before = store.Entries.ToArray(); int calls = store.Calls;
        var forged = remote with { Payload = Row("forged") };
        Assert.Throws<InvalidDataException>(() => owner.Write(Token(2), 1, [forged, resolved], null));
        foreach (var value in new[] { remote with { Payload = Row("forged") }, remote with { Address = 2 },
            CausalVersion.Advance(2, Alice, 8, [], null) })
            Assert.Throws<InvalidDataException>(() => owner.Receive([value]));
        Assert.Throws<InvalidDataException>(() => owner.Receive([remote, remote with { Address = 0 }]));
        Assert.Equal(calls, store.Calls); Assert.Equal(before, store.Entries);
        var clock = new[] { new CausalDot(Bob, 2), new CausalDot(Token(3), 1) };
        var value1 = new CausalVersion(2, clock[0], clock, null); var isolated = Owner(new()); isolated.Receive([value1]);
        Assert.Throws<InvalidDataException>(() => isolated.Receive([value1 with { Dot = clock[1] }]));
        var unreceived = new Store(); var observing = Owner(unreceived);
        var later = CausalVersion.Advance(1, Bob, 1, [remote], Row("later"));
        var mutation = observing.Write(Token(1), 1, [later, remote, remote], null);
        var evidence = unreceived.Entries.ToArray(); int publications = unreceived.Calls;
        Assert.Throws<InvalidDataException>(() => Owner(unreceived).Receive([forged]));
        Assert.Throws<InvalidDataException>(() => Owner(unreceived).Write(Token(1), 1, [forged, later], null));
        Assert.Equal(mutation.Encode(), observing.Write(Token(1), 1, [later], null).Encode());
        Assert.Equal(evidence, unreceived.Entries); Assert.Equal(publications, unreceived.Calls);
    }

    [Fact]
    public void AdmissionAndMalformedSnapshotsPreserveStoredBytes()
    {
        foreach (var options in new[] { new CausalReplicaOptions(MaxEntries: 2), new CausalReplicaOptions(MaxBytes: 180) })
        {
            var store = new Store(); var owner = Owner(store, options: options);
            if (options.MaxEntries == 2) owner.Write(Token(1), 1, [], Row("first"));
            var before = store.Entries.ToArray(); int calls = store.Calls;
            Assert.Throws<InvalidDataException>(() => owner.Write(Token(2), 2, [], Row(new('x', 300))));
            Assert.Equal(calls, store.Calls); Assert.Equal(before, store.Entries);
        }
        var corrupt = new Store(); var reader = Owner(corrupt); reader.Write(Token(1), 1, [], Row("first"));
        var original = corrupt.Entries.ToArray(); int published = corrupt.Calls;
        foreach (string[] entries in new string[][] { [original[0] + " ", original[1]], [original[0], "{"],
            [original[0], original[1] + " "], [original[1]], [original[0], original[1], original[1]] })
        {
            corrupt.Entries = entries; Assert.ThrowsAny<Exception>(() => reader.Snapshot()); Assert.Equal(published, corrupt.Calls);
        }
        corrupt.Entries = original; Assert.Throws<InvalidDataException>(() => Owner(corrupt, Bob).Snapshot());
        foreach (string frame in new[] { "[[[[[[]]]]]]", "\"" + new string('1', 1048575) + "\"",
            "[" + string.Join(",", Enumerable.Repeat("null", 32770)) + "]", "{}", "}", "\"", "[", "]", "\\", "\x1f", "\x7f" })
        {
            corrupt.Entries = [original[0], frame];
            Assert.Throws<InvalidDataException>(() => reader.Snapshot()); Assert.Equal(published, corrupt.Calls);
        }
        corrupt.Entries = original;
        foreach (string entry in new[] { "[\"causal-receive1\",null]", "[\"causal-receive1\",[],null]",
            "[\"causal-receive1\",[" + string.Join(",", Enumerable.Repeat(CausalVersion.Advance(1, Bob, 0, [], null).Encode(), 65)) + "]]" })
        {
            corrupt.Entries = [original[0], entry];
            Assert.Throws<InvalidDataException>(() => reader.Snapshot()); Assert.Equal(published, corrupt.Calls);
        }
        corrupt.Entries = original;
        Assert.Throws<InvalidDataException>(() => reader.Receive(Enumerable.Repeat(CausalVersion.Advance(1, Bob, 0, [], null), 65).ToArray()));
        Assert.Equal(published, corrupt.Calls);
        var clock = Enumerable.Range(2, 64).Select(value => new CausalDot(Token(value), 1)).ToArray();
        var maximum = clock.Select((dot, index) => new CausalVersion((ulong)index + 1, dot,
            clock.ToArray(), Row(new('x', 4096)))).ToArray();
        var large = new Store(); Assert.True(Owner(large).Receive(maximum));
        Assert.Equal(64, Owner(large).Snapshot().Versions.Count);
        Assert.Equal(2, large.Entries.Length); Assert.True(large.Entries[1].Length < 1048576);
    }

    [Fact]
    public void InvalidArgumentsAndProviderShapesRejectAtTheBoundary()
    {
        var store = new Store { SupportsDurableFlush = false };
        Assert.Throws<ArgumentNullException>(() => new CausalReplica(null!, "replica", Alice));
        Assert.Throws<InvalidDataException>(() => new CausalReplica(store, "", Alice));
        Assert.Throws<InvalidDataException>(() => Owner(store, new('0', 32)));
        foreach (string author in new[] { null!, "bad", new string('g', 32), new string('A', 32) })
            Assert.Throws<InvalidDataException>(() => Owner(store, author));
        foreach (string key in new[] { null!, new string('x', 129), "bad/key" })
            Assert.Throws<InvalidDataException>(() => new CausalReplica(store, key, Alice));
        Assert.Throws<InvalidDataException>(() => Owner(store, options: new(MaxEntries: 1)));
        Assert.Throws<InvalidDataException>(() => Owner(store, options: new(MaxBytes: 0)));
        Assert.Throws<InvalidDataException>(() => Owner(store, options: new(RequireDurableFlush: true)));
        var owner = Owner(store); Assert.Empty(owner.Snapshot().Versions);
        Assert.Throws<InvalidDataException>(() => owner.Read(0));
        Assert.Throws<InvalidDataException>(() => owner.Write("invalid", 1, [], null));
        foreach (var snapshot in new IReadOnlyList<JournalVersion>[] { [], [new(0, ["unexpected"])], [new(1, [null!])] })
        { store.Corrupt = snapshot; Assert.Throws<InvalidDataException>(() => owner.Snapshot()); }
        Assert.Equal(0, store.Calls);
    }
}
