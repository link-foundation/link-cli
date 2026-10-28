using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AddressAllocatorTests
{
    private const string Alice = "00000000000000000000000000000001", Bob = "00000000000000000000000000000002";
    private static string Token(int value) => value.ToString("x32", CultureInfo.InvariantCulture);
    private sealed class Store : IAtomicJournalStore
    {
        public bool SupportsDurableFlush { get; set; } = true;
        internal JournalVersion Value = new(0, []);
        internal int Calls, Failure;
        internal Action? Before;
        internal bool NullRead, ReadError;
        internal IReadOnlyList<JournalVersion>? Corrupt;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        {
            if (ReadError) throw new IOException("Read failure.");
            return NullRead ? null! : Corrupt ?? [Value with { Entries = Value.Entries.ToArray() }];
        }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            var before = Before; Before = null; before?.Invoke(); Calls++;
            int failure = Failure; Failure = 0;
            if (failure == 1) throw new IOException("Before publication.");
            if (expected[0].Generation != Value.Generation) return false;
            Assert.Equal(new[] { "addresses" }, keys);
            Value = new(Value.Generation + 1, updates[0]!.ToArray());
            if (failure == 2) throw new IOException("Lost publication reply.");
            return true;
        }
    }
    private static AddressAllocator Allocator(Store store, AddressAllocatorOptions? options = null) => new(store, "addresses", 1, 20, options);
    private static string Bytes(Store store) => JsonSerializer.Serialize(store.Value);
    private static string Text(ulong value) => value.ToString(CultureInfo.InvariantCulture);

    [Fact]
    public void IndependentCanonicalJournalAndAdmissionVectorsPreserveProviderBytes()
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "address-reservations-v1.json")));
        foreach (var fixture in document.RootElement.GetProperty("cases").EnumerateArray())
        {
            var journal = fixture.GetProperty("journal"); var options = fixture.GetProperty("options");
            var store = new Store
            {
                Value = new(ulong.Parse(journal.GetProperty("generation").GetString()!, CultureInfo.InvariantCulture),
                journal.GetProperty("entries").EnumerateArray().Select(v => v.GetString()!).ToArray())
            };
            var owner = new AddressAllocator(store, "addresses", ulong.Parse(fixture.GetProperty("first").GetString()!, CultureInfo.InvariantCulture),
                ulong.Parse(fixture.GetProperty("last").GetString()!, CultureInfo.InvariantCulture),
                new(options.TryGetProperty("maxEntries", out var entries) ? entries.GetInt32() : 4096,
                    options.TryGetProperty("maxBytes", out var bytes) ? bytes.GetInt32() : 1_048_576));
            string before = Bytes(store);
            if (fixture.GetProperty("valid").GetBoolean())
            {
                var result = owner.Snapshot();
                Assert.Equal(journal.GetProperty("generation").GetString(), Text(result.Generation));
                Assert.Equal(fixture.GetProperty("next").GetString(), result.Next is { } next ? Text(next) : null);
                Assert.Equal(fixture.GetProperty("reservations").EnumerateArray().Select(v => new AddressReservation(
                    v.GetProperty("author").GetString()!, v.GetProperty("token").GetString()!,
                    ulong.Parse(v.GetProperty("count").GetString()!, CultureInfo.InvariantCulture),
                    ulong.Parse(v.GetProperty("first").GetString()!, CultureInfo.InvariantCulture),
                    ulong.Parse(v.GetProperty("last").GetString()!, CultureInfo.InvariantCulture))), result.Reservations);
            }
            else Assert.ThrowsAny<Exception>(() => owner.Snapshot());
            Assert.Equal(before, Bytes(store)); Assert.Equal(0, store.Calls);
        }
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public void ExactRetrySurvivesUncertainRepliesAndFollowingGrants(int failure)
    {
        var store = new Store { Failure = failure };
        if (failure == 0) Allocator(store).Reserve(Alice, Token(10), 3);
        else Assert.Contains("Indeterminate", Assert.Throws<IOException>(() => Allocator(store).Reserve(Alice, Token(10), 3)).Message);
        Assert.Equal(1, store.Calls); if (failure == 1) Assert.Equal(0UL, store.Value.Generation);
        var first = Allocator(store).Reserve(Alice, Token(10), 3);
        Assert.Equal(new AddressReservation(Alice, Token(10), 3, 1, 3), first);
        Assert.Equal(4UL, Allocator(store).Reserve(Bob, Token(20), 2).First);
        string before = Bytes(store); int calls = store.Calls;
        Assert.Equal(first, Allocator(store).Reserve(Alice, Token(10), 3));
        Assert.Throws<InvalidDataException>(() => Allocator(store).Reserve(Bob, Token(10), 3));
        Assert.Throws<InvalidDataException>(() => Allocator(store).Reserve(Alice, Token(10), 2));
        var snapshot = Allocator(store).Snapshot(); ((AddressReservation[])snapshot.Reservations)[0] = first with { Last = 19 };
        Assert.Equal(3UL, Allocator(store).Snapshot().Reservations[0].Last);
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
    }

    [Fact]
    public void SameBaseRaceConflictsThenExplicitRetryGetsDisjointRange()
    {
        var store = new Store(); store.Before = () => Allocator(store).Reserve(Bob, Bob, 2);
        Assert.Contains("Conflict", Assert.Throws<InvalidOperationException>(() => Allocator(store).Reserve(Alice, Alice, 2)).Message);
        Assert.Equal(3UL, Allocator(store).Reserve(Alice, Alice, 2).First);
        Assert.Equal(1UL, Allocator(store).Reserve(Bob, Bob, 2).First);
        Assert.Equal(new[] { (1UL, 2UL), (3UL, 4UL) }, Allocator(store).Snapshot().Reservations.Select(v => (v.First, v.Last)));
    }

    [Fact]
    public void FullUnsignedWidthExhaustionDoesNotCycleOrRecycle()
    {
        var store = new Store(); var owner = new AddressAllocator(store, "addresses", ulong.MaxValue - 1, ulong.MaxValue);
        Assert.Equal(ulong.MaxValue - 1, owner.Reserve(Alice, Alice, 1).First);
        Assert.Equal(ulong.MaxValue, owner.Reserve(Bob, Bob, 1).Last);
        string before = Bytes(store); int calls = store.Calls;
        Assert.Null(owner.Snapshot().Next); Assert.Throws<InvalidDataException>(() => owner.Reserve(Alice, Token(3), 1));
        Assert.Equal(ulong.MaxValue, owner.Reserve(Bob, Bob, 1).Last);
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
        var whole = new AddressAllocator(new Store(), "addresses", 1, ulong.MaxValue);
        Assert.Equal(ulong.MaxValue, whole.Reserve(Alice, Alice, ulong.MaxValue).Last); Assert.Null(whole.Snapshot().Next);
    }

    [Theory]
    [InlineData(2, 1_048_576)]
    [InlineData(4096, 170)]
    public void FiniteAdmissionAllowsOldRetriesWithoutPublication(int maxEntries, int maxBytes)
    {
        var store = new Store(); var owner = Allocator(store, new(maxEntries, maxBytes));
        owner.Reserve(Alice, Alice, 1); string before = Bytes(store); int calls = store.Calls;
        Assert.Throws<InvalidDataException>(() => owner.Reserve(Bob, Bob, 1)); Assert.Equal(1UL, owner.Reserve(Alice, Alice, 1).First);
        Assert.Equal(before, Bytes(store)); Assert.Equal(calls, store.Calls);
        var tiny = new Store(); Assert.Throws<InvalidDataException>(() => Allocator(tiny, new(MaxBytes: 1)).Reserve(Alice, Alice, 1)); Assert.Equal(0, tiny.Calls);
    }

    [Fact]
    public void BadConfigurationRequestsAndProviderFramesCannotPublish()
    {
        var store = new Store();
        foreach (var (first, last) in new[] { (0UL, 1UL), (2UL, 1UL) }) Assert.Throws<InvalidDataException>(() => new AddressAllocator(store, "addresses", first, last));
        foreach (string key in new[] { null!, "", "bad/key", new string('x', 129) }) Assert.Throws<InvalidDataException>(() => new AddressAllocator(store, key, 1, 20));
        Assert.Throws<InvalidDataException>(() => new AddressAllocator(null!, "addresses", 1, 20));
        foreach (var options in new[] { new AddressAllocatorOptions(MaxEntries: 1), new(MaxEntries: 4097), new(MaxBytes: 0), new(MaxBytes: 1_048_577) })
            Assert.Throws<InvalidDataException>(() => Allocator(store, options));
        _ = new AddressAllocator(store, ".valid_-09Z", 1, 20, new(RequireDurableFlush: true));
        store.SupportsDurableFlush = false; Assert.Throws<InvalidDataException>(() => Allocator(store, new(RequireDurableFlush: true)));
        var owner = Allocator(store);
        foreach (var (author, token, count) in new[] { (null!, Alice, 1UL), ("", Alice, 1UL), (new string('0', 32), Alice, 1UL), (new string('F', 32), Alice, 1UL), (Alice, "bad", 1UL), (Alice, Alice, 0UL) })
            Assert.Throws<InvalidDataException>(() => owner.Reserve(author, token, count));
        foreach (IReadOnlyList<JournalVersion> reply in new IReadOnlyList<JournalVersion>[] { [], [null!], [new(0, []), new(0, [])], [new(0, null!)], [new(1, [null!])] })
        { store.Corrupt = reply; Assert.Throws<InvalidDataException>(() => owner.Snapshot()); }
        store.Corrupt = null; store.NullRead = true; Assert.Throws<InvalidDataException>(() => owner.Snapshot()); store.NullRead = false;
        store.ReadError = true; Assert.Throws<IOException>(() => owner.Reserve(Alice, Alice, 1)); store.ReadError = false;
        store.Value = new(ulong.MaxValue, ["[\"address-reservations1\",\"1\",\"20\"]"]);
        Assert.Throws<InvalidDataException>(() => owner.Reserve(Alice, Alice, 1)); Assert.Equal(0, store.Calls);
    }
}
