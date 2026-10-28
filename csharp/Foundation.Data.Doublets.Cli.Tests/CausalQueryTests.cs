using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed partial class CausalQueryTests
{
    private const string Alice = "00000000000000000000000000000001";
    private static string Token(int number) => number.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalBatch Batch(Store store) => new(store, "replica", "batches", Alice);
    private static CausalSession Session(Store store, ulong first = 1, ulong last = 8) => new(Batch(store), first, last, CausalConflictPolicy.MaxDot);
    private static string Bytes(Store store) => JsonSerializer.Serialize(store.Values.OrderBy(value => value.Key));
    private static string[] Wires(IEnumerable<CausalVersion> versions) => versions.Select(value => value.Encode()).ToArray();
    private static ulong Number(JsonElement value, string key) => ulong.Parse(value.GetProperty(key).GetString()!, CultureInfo.InvariantCulture);
    private static string? Name(JsonElement row) => row.GetProperty("name").GetString();
    private static void Query(INamedTypesLinks<UInt128> links, string text) => AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(links, text);

    public static IEnumerable<object[]> Vectors()
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "causal-query-v1.json")));
        return document.RootElement.GetProperty("cases").EnumerateArray().Select(value => new object[]
        { value.GetProperty("name").GetString()!, value.GetRawText() }).ToArray();
    }

    [Theory]
    [MemberData(nameof(Vectors))]
    public void SharedQueriesStayPrivateAndPublishOneExactlyRetryableBatch(string name, string json)
        => CheckVector(name, json, Query);

    private static void CheckVector(string name, string json, Action<INamedTypesLinks<UInt128>, string> execute)
    {
        using var document = JsonDocument.Parse(json);
        var vector = document.RootElement;
        var store = new Store();
        var seed = vector.GetProperty("seed").EnumerateArray().Select(row => new CausalBatchEdit(
            Number(row, "index"), [], new(Number(row, "source"), Number(row, "target"), Name(row)))).ToArray();
        if (seed.Length > 0) Batch(store).Write(Token(10), 0, seed);
        var before = Bytes(store);
        using var tx = Session(store, Number(vector, "first"), Number(vector, "last"));
        var links = tx.QueryLinks;
        Assert.Same(links, tx.QueryLinks);
        foreach (var query in vector.GetProperty("queries").EnumerateArray())
        {
            var error = Record.Exception(() => execute(links, query.GetProperty("text").GetString()!));
            Assert.True((error is not null) == query.GetProperty("error").GetBoolean(), $"{name}: {error}");
        }
        var actual = tx.Rows().Select(row => (row.Link.Index, row.Link.Source, row.Link.Target, row.Name));
        var expected = vector.GetProperty("rows").EnumerateArray().Select(row =>
            (Number(row, "index"), Number(row, "source"), Number(row, "target"), Name(row)));
        Assert.Equal(expected, actual);
        UInt128 allocated = 0;
        links.Create(null, (_, row) => { allocated = row![0]; return links.Constants.Continue; });
        Assert.Equal((UInt128)Number(vector, "next"), allocated);
        links.Delete([allocated], null);
        Assert.Equal(before, Bytes(store));
        Assert.Equal(vector.GetProperty("edits").GetInt32(), tx.Prepare(Token(100)).Edits.Count);
        Assert.Throws<InvalidOperationException>(() => links.Count(null));
        var result = Wires(tx.Commit(Token(100)));
        var committed = Bytes(store);
        Assert.Equal(result, Wires(tx.Commit(Token(100))));
        Assert.Equal(committed, Bytes(store));
    }

    [Fact]
    public void FullStoredMaximumIsConcreteAndEveryObservationIsOwned()
    {
        var store = new Store();
        Batch(store).Write(Token(10), 0, [new(100, [], new(100, 100, null)), new(ulong.MaxValue, [], new(0, ulong.MaxValue, "wide"))]);
        using var tx = Session(store);
        var links = tx.QueryLinks;
        Assert.True(links.Constants.Any > ulong.MaxValue);
        Assert.Equal((UInt128)1, links.Count([ulong.MaxValue]));
        Assert.Equal((UInt128)2, links.Count([links.Constants.Any]));
        IList<UInt128>? observed = null;
        links.Each([ulong.MaxValue], row => { observed = row; return links.Constants.Break; });
        Assert.Equal(new UInt128[] { ulong.MaxValue, 0, ulong.MaxValue }, observed!);
        Assert.Throws<NotSupportedException>(() => observed![0] = 1);
        links.Update([ulong.MaxValue], [ulong.MaxValue, ulong.MaxValue, 0], null);
        Assert.Equal(new UInt128[] { ulong.MaxValue, 0, ulong.MaxValue }, observed!);
        Assert.Equal((UInt128)1, links.Count([ulong.MaxValue, 0]));
        Assert.Equal((UInt128)0, links.Count([100, 0]));
        Assert.Equal((UInt128)1, links.Count([ulong.MaxValue, ulong.MaxValue, 0]));
        UInt128 created = 0;
        links.Create(null, (_, row) => { created = row![0]; return links.Constants.Continue; });
        Assert.Equal(UInt128.One, created);
    }

    [Fact]
    public void ResurrectingRetainedMaximumDoesNotAdvanceTheLowerFreshRange()
    {
        var store = new Store();
        Batch(store).Write(Token(10), 0, [new(ulong.MaxValue, [], null)]);
        using var tx = Session(store);
        Query(tx.QueryLinks, "() ((18446744073709551615: 18446744073709551615 18446744073709551615) (1 1))");
        Assert.Equal(new[] { 1UL, ulong.MaxValue }, tx.Rows().Select(row => row.Link.Index));
        Assert.Equal(2UL, tx.CreateLink(0, 0));
    }

    [Theory]
    [InlineData("-1")]
    [InlineData("-0")]
    [InlineData("18446744073709551616")]
    [InlineData("340282366920938463463374607431768211456")]
    [InlineData("100")]
    public void InvalidCheckedDefinitionLeavesEveryPrivateRowAndCursorUntouched(string id)
    {
        var store = new Store();
        using var tx = Session(store);
        var before = Bytes(store);
        Assert.ThrowsAny<Exception>(() => Query(tx.QueryLinks, $"() ((1: 1 1) ({id}: 0 0))"));
        Assert.Empty(tx.Rows());
        Assert.Equal(before, Bytes(store));
        Assert.Equal(1UL, tx.CreateLink(0, 0));
    }

    [Fact]
    public void RetainedCheckedAdapterRejectsEveryReadAndWriteAfterPreparationOrDisposal()
    {
        foreach (var prepare in new[] { false, true })
        {
            using var tx = Session(new Store());
            var links = tx.QueryLinks;
            if (prepare) tx.Prepare(Token(100)); else tx.Dispose();
            Action[] calls = [() => _ = tx.QueryLinks, () => links.Count(null), () => links.Each(null, null),
                () => links.GetName(0), () => links.GetByName("name"), () => links.SetName(1, "name"),
                () => links.RemoveName(1), () => links.Create(null, null), () => links.Update([1], [1, 0, 0], null),
                () => links.Delete([1], null), () => links.TryEnsureCreated(1),
                () => links.ReserveQueryAllocations(_ => { }), () => links.CaptureNames(),
                () => Query(links, ""), () => AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(links, new() { Query = null })];
            foreach (var call in calls) Assert.Throws<InvalidOperationException>(call);
        }
    }

    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    public void QueryPreparationRetriesTheOriginalTokenAfterUnknownPublicationAndFollowingWrites(int failure)
    {
        var store = new Store();
        using var tx = Session(store);
        Query(tx.QueryLinks, "() ((1 1))");
        store.Failure = failure;
        Assert.Throws<IOException>(() => tx.Commit(Token(100)));
        Assert.Throws<InvalidOperationException>(() => Query(tx.QueryLinks, ""));
        Assert.Throws<InvalidOperationException>(() => tx.Commit(Token(200)));
        if (failure == 2) new CausalReplica(store, "replica", Alice).Write(Token(300), 100, [], new(0, 0, "following"));
        var result = Wires(tx.Commit(Token(100)));
        var before = Bytes(store);
        Assert.Equal(result, Wires(tx.Commit(Token(100))));
        Assert.Equal(before, Bytes(store));
    }

    private sealed class Store : IAtomicJournalStore
    {
        internal readonly Dictionary<string, JournalVersion> Values = new(StringComparer.Ordinal);
        internal int Failure;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => keys.Select(key =>
            Values.TryGetValue(key, out var value) ? value with { Entries = value.Entries.ToArray() } : new JournalVersion(0, [])).ToArray();
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            var failure = Failure; Failure = 0;
            if (failure == 1) throw new IOException("Before publication.");
            if (keys.Where((key, i) => Values.GetValueOrDefault(key)?.Generation != expected[i].Generation &&
                !(Values.GetValueOrDefault(key) is null && expected[i].Generation == 0)).Any()) return false;
            for (var i = 0; i < keys.Count; i++) Values[keys[i]] = new(expected[i].Generation + 1, updates[i]!.ToArray());
            if (failure == 2) throw new IOException("Lost publication reply.");
            return true;
        }
    }
}
