using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;
using Xunit;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AuthorFenceTests
{
    private const string A = "00000000000000000000000000000001";
    private const string T1 = "00000000000000000000000000000010", T2 = "00000000000000000000000000000020";
    private sealed class Store : IAtomicJournalStore
    {
        internal Dictionary<string, JournalVersion> Values = new();
        internal int Calls, Failure; internal Action? Before;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => keys.Select(k => Values.TryGetValue(k, out var v) ? v with { Entries = v.Entries.ToArray() } : new JournalVersion(0, [])).ToArray();
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            var before = Before; Before = null; before?.Invoke(); Calls++;
            int failure = Failure; Failure = 0; if (failure == 1) throw new IOException("before");
            if (keys.Where((key, i) => (Values.TryGetValue(key, out var v) ? v.Generation : 0) != expected[i].Generation).Any()) return false;
            for (int i = 0; i < keys.Count; i++) if (updates[i] is { } update) Values[keys[i]] = new(expected[i].Generation + 1, update.ToArray());
            if (failure == 2) throw new IOException("after"); return true;
        }
        internal string Bytes() => JsonSerializer.Serialize(Values);
    }
    [Fact]
    public void CanonicalFixtures()
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "author-fence-v1.json")));
        foreach (var vector in doc.RootElement.GetProperty("cases").EnumerateArray())
        {
            var s = new Store(); var v = vector.GetProperty("journal");
            s.Values["fence"] = new(ulong.Parse(v.GetProperty("generation").GetString()!), v.GetProperty("entries").EnumerateArray().Select(e => e.GetString()!).ToArray());
            var f = new AuthorFence(s, "fence", A); string before = s.Bytes();
            if (vector.GetProperty("valid").GetBoolean())
            {
                var snapshot = f.Snapshot(); Assert.Equal(ulong.Parse(vector.GetProperty("epoch").GetString()!), snapshot.Epoch);
                Assert.Equal(vector.GetProperty("claims").GetArrayLength(), snapshot.Claims.Count);
            }
            else Assert.ThrowsAny<Exception>(() => f.Snapshot());
            Assert.Equal(before, s.Bytes()); Assert.Equal(0, s.Calls);
        }
    }
    [Fact]
    public void TakeoverFencesReadsWritesAndOldRetries()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A); var first = f.Claim(0, T1); var old = f.Bind(first);
        var expected = old.Read(["data"]); Assert.True(old.CompareExchange(["data"], expected, [new[] { "one" }]));
        var current = f.Bind(f.Claim(1, T2)); string before = s.Bytes(); int calls = s.Calls;
        Assert.ThrowsAny<Exception>(() => old.Read(["data"]));
        Assert.ThrowsAny<Exception>(() => old.CompareExchange(["data"], expected, [new[] { "stale" }]));
        Assert.ThrowsAny<Exception>(() => f.Claim(0, T1)); Assert.Equal(before, s.Bytes()); Assert.Equal(calls, s.Calls);
        Assert.Equal("one", current.Read(["data"])[0].Entries.Single()); Assert.Same(s, current.Store);
    }
    [Fact]
    public void TakeoverBetweenFenceReadAndDataCasIsAtomic()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A); var old = f.Bind(f.Claim(0, T1)); var expected = old.Read(["a", "b"]);
        s.Before = () => f.Claim(1, T2);
        Assert.False(old.CompareExchange(["a", "b"], expected, [new[] { "a" }, new[] { "b" }]));
        Assert.False(s.Values.ContainsKey("a")); Assert.False(s.Values.ContainsKey("b"));
        Assert.ThrowsAny<Exception>(() => old.CompareExchange(["a", "b"], expected, [new[] { "a" }, new[] { "b" }]));
    }
    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    public void LostRepliesRetryOnlyTheOriginalActiveClaim(int failure)
    {
        var s = new Store { Failure = failure }; var f = new AuthorFence(s, "fence", A);
        Assert.Throws<IOException>(() => f.Claim(0, T1)); Assert.Equal(1, s.Calls);
        var first = f.Claim(0, T1); Assert.Equal(1UL, first.Epoch); f.Claim(1, T2);
        string before = s.Bytes(); Assert.ThrowsAny<Exception>(() => f.Claim(0, T1)); Assert.Equal(before, s.Bytes());
    }
    [Fact]
    public void ReadOnlyGuardAndFiniteReceiptAdmission()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A, new(MaxEntries: 2)); var first = f.Claim(0, T1); var store = f.Bind(first);
        string before = s.Bytes(); Assert.True(store.CompareExchange(["data"], store.Read(["data"]), [null])); Assert.Equal(before, s.Bytes());
        Assert.ThrowsAny<Exception>(() => f.Claim(1, T2)); Assert.Equal(first, f.Claim(0, T1)); Assert.Equal(before, s.Bytes());
    }
    [Fact]
    public void MalformedAdmissionCannotPublish()
    {
        var s = new Store();
        Assert.ThrowsAny<Exception>(() => new AuthorFence(s, "bad/key", A));
        Assert.ThrowsAny<Exception>(() => new AuthorFence(s, "fence", "bad"));
        Assert.ThrowsAny<Exception>(() => new AuthorFence(s, "fence", A, new(MaxEntries: 1)));
        Assert.ThrowsAny<Exception>(() => new AuthorFence(s, "fence", A, new(MaxBytes: 0)));
        var f = new AuthorFence(s, "fence", A);
        Assert.ThrowsAny<Exception>(() => f.Claim(1, T1)); Assert.ThrowsAny<Exception>(() => f.Claim(0, "bad"));
        Assert.ThrowsAny<Exception>(() => f.Bind(new(A, T1, 0)));
        var current = f.Bind(f.Claim(0, T1)); string before = s.Bytes(); int calls = s.Calls;
        foreach (var keys in new[] { Array.Empty<string>(), new[] { "fence" }, new[] { "x", "x" }, Enumerable.Range(0, 32).Select(i => $"x{i}").ToArray() })
            Assert.ThrowsAny<Exception>(() => current.Read(keys));
        Assert.ThrowsAny<Exception>(() => current.CompareExchange(["data"], [], []));
        Assert.Equal(before, s.Bytes()); Assert.Equal(calls, s.Calls);
    }
    [Fact]
    public void TakeoverResolvesDecidedBatchAndBurnsOldReservationSuffix()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A); var first = f.Claim(0, T1); var guarded = f.Bind(first);
        var allocator = new AddressAllocator(guarded, "addresses", 1, 100);
        Assert.Equal(4UL, allocator.Reserve(A, "00000000000000000000000000000110", 4).Last);
        var old = new CausalBatch(guarded, "owner", "receipts", A);
        CausalBatchEdit[] edits = [new(1, [], new(0, 0, null))]; s.Failure = 2;
        Assert.Throws<IOException>(() => old.Write("00000000000000000000000000000300", 0, edits));
        var next = f.Bind(f.Claim(1, T2)); string before = s.Bytes();
        Assert.ThrowsAny<Exception>(() => old.Write("00000000000000000000000000000300", 0, edits)); Assert.Equal(before, s.Bytes());
        var current = new CausalBatch(next, "owner", "receipts", A);
        Assert.Equal(1UL, current.Write("00000000000000000000000000000300", 0, edits)[0].Dot.Counter);
        var grant = new AddressAllocator(next, "addresses", 1, 100).Reserve(A, "00000000000000000000000000000210", 2);
        Assert.Equal(5UL, grant.First); var epoch = s.Values["fence"];
        current.Write("00000000000000000000000000000400", current.Snapshot().Generation, [new(grant.First, [], new(0, 0, null))]);
        Assert.Equal(2UL, current.Snapshot().LastCounter); Assert.Equal(epoch, s.Values["fence"]);
    }
}
