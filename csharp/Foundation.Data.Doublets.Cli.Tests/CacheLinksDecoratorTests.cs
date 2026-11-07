using Platform.Data;
using Platform.Data.Doublets;
using Stores = Foundation.Data.Doublets.Cli.Tests.UnionAndRemapLinksTests.Stores;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>A store kept as a cache. The Rust twin is <c>rust/tests/cache_links_tests.rs</c>.</summary>
public sealed class CacheLinksDecoratorTests
{
    private sealed class Harness : IDisposable
    {
        private readonly Stores _stores = new();
        public ulong Now = 1000;
        public NamedTypesDecorator<uint> Below { get; }
        public CacheLinksDecorator<uint> Cache { get; private set; }

        public Harness(CacheOptions? options = null)
        {
            Below = _stores.New();
            Cache = Over(options);
        }

        public CacheLinksDecorator<uint> Over(CacheOptions? options) =>
            Cache = new CacheLinksDecorator<uint>(Below, options, () => Now);

        public uint Make(uint source, uint target) => UnionAndRemapLinksTests.Make(Cache, source, target);

        public List<uint> Addresses() =>
            UnionAndRemapLinksTests.All(Cache).Select(link => link.Item1).ToList();

        public void Dispose() => _stores.Dispose();
    }

    private static uint Total(ILinks<uint> links) => links.Count();

    private static List<uint> Sorted(params uint[] addresses) => addresses.OrderBy(address => address).ToList();

    [Fact]
    public void ACacheWithNoOptionsIsTheStoreBelow()
    {
        using var harness = new Harness();
        var link = harness.Make(1, 1);
        harness.Now = ulong.MaxValue;
        Assert.True(harness.Cache.Exists(link));
        Assert.Null(harness.Cache.TimeToLive(link));
        Assert.Equal(0, harness.Cache.PurgeExpired());
        Assert.Equal(((INamedTypesLinks<uint>)harness.Below).ReassignsNames, harness.Cache.ReassignsNames);
        Assert.Null(harness.Cache.QueryLimits);
        Assert.True(harness.Cache.Claims(99));
    }

    [Fact]
    public void TheSystemClockIsTheDefaultSoALongDeadlineHasNotPassed()
    {
        using var stores = new Stores();
        var cache = new CacheLinksDecorator<uint>(stores.New());
        var link = UnionAndRemapLinksTests.Make(cache, 1, 1);
        Assert.True(cache.Expire(link, 3_600_000));
        Assert.True(cache.TimeToLive(link) > 3_000_000);
    }

    [Fact]
    public void ALinkGoesAtItsDeadlineAndNotBefore()
    {
        using var harness = new Harness(new CacheOptions(TimeToLive: 100));
        var link = harness.Make(1, 1);
        Assert.Equal(100ul, harness.Cache.TimeToLive(link));

        harness.Now = 1099;
        Assert.True(harness.Cache.Exists(link));
        Assert.Equal(1ul, harness.Cache.TimeToLive(link));

        harness.Now = 1100;
        Assert.False(harness.Cache.Exists(link));
        Assert.Null(harness.Cache.TimeToLive(link));
    }

    [Fact]
    public void EveryReadAndWriteSeesWhatCameDueAsGone()
    {
        var checks = new Action<CacheLinksDecorator<uint>, uint>[]
        {
            (cache, link) => Assert.False(cache.Exists(link)),
            (cache, _) => Assert.Equal(0u, Total(cache)),
            (cache, _) => Assert.Empty(cache.All()),
            (cache, link) => Assert.Null(cache.GetName(link)),
            (cache, _) => Assert.Equal(cache.Constants.Null, cache.GetByName("a")),
            (cache, _) => Assert.Empty(cache.CaptureNames()),
            (cache, _) => cache.PurgeExpired(),
        };
        foreach (var check in checks)
        {
            using var harness = new Harness(new CacheOptions(TimeToLive: 5));
            var link = harness.Make(1, 1);
            harness.Cache.SetName(link, "a");
            harness.Now += 5;
            check(harness.Cache, link);
            Assert.Equal(0u, Total(harness.Below));
        }
    }

    [Fact]
    public void ANameIsTheStoresToKeep()
    {
        using var harness = new Harness();
        var link = harness.Make(1, 1);
        harness.Cache.SetName(link, "a");
        Assert.Equal("a", harness.Cache.GetName(link));
        Assert.Equal(link, harness.Cache.GetByName("a"));
        Assert.Equal("a", harness.Cache.CaptureNames()[link]);
        harness.Cache.RemoveName(link);
        Assert.Equal(harness.Cache.Constants.Null, harness.Cache.GetByName("a"));
    }

    [Fact]
    public void ADeadlineCanBeReplacedTakenAwayAndRefusedForWhatIsAbsent()
    {
        using var harness = new Harness();
        var cache = harness.Cache;
        var link = harness.Make(1, 1);

        Assert.False(cache.Persist(link));
        Assert.False(cache.Persist(99));
        Assert.False(cache.Expire(99, 1));
        Assert.Null(cache.TimeToLive(99));

        Assert.True(cache.Expire(link, 10));
        Assert.True(cache.Expire(link, 20));
        harness.Now += 10;
        Assert.Equal(10ul, cache.TimeToLive(link));

        Assert.True(cache.Persist(link));
        harness.Now += 100;
        Assert.True(cache.Exists(link));
        Assert.Null(cache.TimeToLive(link));

        // The longest time there is does not wrap around to a past deadline.
        Assert.True(cache.Expire(link, ulong.MaxValue));
        Assert.True(cache.Exists(link));
    }

    [Fact]
    public void ADefaultTimeToLiveNearTheEndOfTimeDoesNotWrap()
    {
        using var harness = new Harness(new CacheOptions(TimeToLive: ulong.MaxValue));
        var link = harness.Make(1, 1);
        Assert.True(harness.Cache.Exists(link));
    }

    [Fact]
    public void AnExpiryOfZeroDeletesAtOnce()
    {
        using var harness = new Harness();
        var link = harness.Make(1, 1);
        Assert.True(harness.Cache.Expire(link, 0));
        Assert.Equal(0u, Total(harness.Below));
    }

    [Fact]
    public void WhatComesDueTakesAlongWhatIsBuiltOnItAndCountsIt()
    {
        using var harness = new Harness();
        var cache = harness.Cache;
        var baseLink = harness.Make(50, 50);
        var built = harness.Make(baseLink, baseLink);
        var other = harness.Make(3, 3);

        Assert.True(cache.Expire(baseLink, 5));
        Assert.True(cache.Expire(built, 50));
        harness.Now += 5;
        Assert.Equal(2, cache.PurgeExpired());
        Assert.Equal(Sorted(other), harness.Addresses());

        // The deadline of what went along went too.
        harness.Now += 100;
        Assert.Equal(0, cache.PurgeExpired());
    }

    [Fact]
    public void CapacityEvictsTheLeastRecentlyUsedLink()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 2));
        var cache = harness.Cache;
        var first = harness.Make(1, 1);
        var second = harness.Make(2, 2);
        var third = harness.Make(3, 3);
        Assert.Equal(Sorted(second, third), harness.Addresses());
        Assert.NotEqual(0u, first);

        // Reading a link and finding it both count as using it, a scan does not.
        Assert.NotNull(cache.GetLink(second));
        Assert.True(cache.Exists(third));
        var fourth = harness.Make(4, 4);
        Assert.Equal(Sorted(second, fourth), harness.Addresses());

        Assert.Equal(second, cache.SearchOrDefault(2u, 2u));
        var fifth = harness.Make(5, 5);
        Assert.Equal(Sorted(second, fifth), harness.Addresses());
    }

    [Fact]
    public void ALinkInUseIsNotEvictedAndAFullCacheRefuses()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 2));
        var baseLink = harness.Make(50, 50);
        var built = harness.Make(baseLink, baseLink);

        // Only `built` can go, and it does, because nothing uses it.
        var other = harness.Make(3, 3);
        Assert.Equal(Sorted(baseLink, other), harness.Addresses());
        Assert.NotEqual(0u, built);

        // The link a create refers to is kept for it, and `other` goes in its place.
        var again = harness.Make(baseLink, 9);
        Assert.Equal(Sorted(baseLink, again), harness.Addresses());

        // Both are named by the next link, so nothing is left to evict, and it goes again.
        var refused = Assert.Throws<InvalidOperationException>(() => harness.Make(again, baseLink));
        Assert.Contains("holds 2 links", refused.Message);
        Assert.Equal(Sorted(baseLink, again), harness.Addresses());
    }

    [Fact]
    public void ALinkTwoOthersUseIsHeldUntilBothAreGone()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 3));
        var baseLink = harness.Make(50, 50);
        var first = harness.Make(baseLink, baseLink);
        var second = harness.Make(baseLink, 5);
        harness.Cache.Delete(first);
        harness.Cache.Delete(second);
        var a = harness.Make(7, 7);
        var b = harness.Make(8, 8);
        var c = harness.Make(9, 9);
        Assert.Equal(Sorted(a, b, c), harness.Addresses());
    }

    [Fact]
    public void UpdatingMovesWhatALinkUses()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 3));
        var oldLink = harness.Make(50, 50);
        var newLink = harness.Make(2, 2);
        var user = harness.Make(oldLink, oldLink);
        ((ILinks<uint>)harness.Cache).Update(user, newLink, 6u);

        // `old` is free now and the oldest, `new` is held.
        var other = harness.Make(4, 4);
        Assert.Equal(Sorted(newLink, user, other), harness.Addresses());
    }

    [Fact]
    public void ACacheHoldingNothingRefusesEverything()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 0));
        Assert.Throws<InvalidOperationException>(() => harness.Cache.Create());
        Assert.Throws<InvalidOperationException>(() => harness.Cache.EnsureCreated(5u));
        Assert.Equal(0u, Total(harness.Below));
    }

    [Fact]
    public void LinksAlreadyThereAreTheFirstToGoAndHaveNoDeadline()
    {
        using var harness = new Harness();
        var first = UnionAndRemapLinksTests.Make(harness.Below, 50, 50);
        var second = UnionAndRemapLinksTests.Make(harness.Below, first, first);
        var third = UnionAndRemapLinksTests.Make(harness.Below, 3, 3);
        var cache = harness.Over(new CacheOptions(MaxLinks: 3, TimeToLive: 5));
        Assert.Null(cache.TimeToLive(first));
        var created = harness.Make(4, 4);
        Assert.Equal(Sorted(first, third, created), harness.Addresses());
        Assert.NotEqual(0u, second);

        // Only the new link has a deadline.
        harness.Now += 5;
        Assert.Equal(Sorted(first, third), harness.Addresses());
    }

    [Fact]
    public void CreatingAtAnAddressIsTrackedAndMakesRoom()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 2));
        var cache = harness.Cache;
        var first = harness.Make(1, 1);
        cache.EnsureCreated(first, cache.Constants.Any);
        Assert.Equal(Sorted(first), harness.Addresses());

        cache.EnsureCreated(10u);
        Assert.Equal(Sorted(first, 10), harness.Addresses());
        cache.EnsureCreated(11u);
        Assert.Equal(Sorted(10, 11), harness.Addresses());
        Assert.True(cache.Expire(11u, 1000));

        // Addresses named together are not evicted for each other.
        Assert.Throws<InvalidOperationException>(() => cache.EnsureCreated(10u, 11u, 12u));
        Assert.Equal(Sorted(10, 11), harness.Addresses());
    }

    [Fact]
    public void CreatingAtAnAddressWithNoLimitOnlyCreates()
    {
        using var harness = new Harness();
        harness.Cache.EnsureCreated(3u);
        Assert.Equal(Sorted(3), harness.Addresses());
    }

    [Fact]
    public void AnEmptyLinkIsCountedOverTheLimitUntilTheNextWrite()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 1));
        var cache = harness.Cache;
        var first = harness.Make(1, 1);
        var empty = cache.Create();
        Assert.Equal(2u, Total(cache));

        // Any other write settles it: `first` goes, the empty link stays.
        var unrelated = harness.Cache.Create();
        Assert.Equal(Sorted(empty, unrelated), harness.Addresses());
        Assert.NotEqual(0u, first);
    }

    [Fact]
    public void AnEmptyLinkIsSettledByADeleteAndAnUpdateOfAnother()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 2));
        var cache = harness.Cache;
        var first = harness.Make(1, 1);
        var second = harness.Make(2, 2);
        var empty = cache.Create();
        ((ILinks<uint>)cache).Update(second, 5u, 5u);
        Assert.Equal(Sorted(second, empty), harness.Addresses());
        Assert.NotEqual(0u, first);

        var another = cache.Create();

        // The new link is the one kept: the settling evicts `empty`, the older of the free ones.
        cache.Delete(second);
        Assert.Equal(Sorted(another), harness.Addresses());
    }

    [Fact]
    public void AnUpdateThatMergesIntoADuplicateIsFollowed()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 3));
        var cache = harness.Cache;
        var keep = harness.Make(1, 1);
        var merged = harness.Make(2, 2);
        var reports = 0;
        cache.Update(new uint[] { merged, cache.Constants.Any, cache.Constants.Any }, new uint[] { merged, 1, 1 }, (_, _) =>
        {
            reports++;
            return cache.Constants.Continue;
        });
        Assert.True(reports > 0);
        Assert.Equal(Sorted(keep), harness.Addresses());
        var a = harness.Make(3, 3);
        var b = harness.Make(4, 4);
        Assert.Equal(Sorted(keep, a, b), harness.Addresses());
    }

    [Fact]
    public void ADeleteReportsToTheCallerAsWell()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 3));
        var cache = harness.Cache;
        var baseLink = harness.Make(50, 50);
        var built = harness.Make(baseLink, baseLink);
        var deleted = new List<uint>();
        cache.Delete(new uint[] { baseLink }, (before, after) =>
        {
            if (after is null || after.Count == 0 || after[0] == 0)
            {
                deleted.Add(before![0]);
            }
            return cache.Constants.Continue;
        });
        Assert.Equal(Sorted(baseLink, built), deleted.OrderBy(address => address).ToList());
    }

    [Fact]
    public void AReadWithNoHandlerStillUsesWhatItNames()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 2));
        var first = harness.Make(1, 1);
        var second = harness.Make(2, 2);
        harness.Cache.Each(new uint[] { first, harness.Cache.Constants.Any, harness.Cache.Constants.Any }, null);
        var third = harness.Make(3, 3);
        Assert.Equal(Sorted(first, third), harness.Addresses());
        Assert.NotEqual(0u, second);

        // No restriction at all, or one with no source or target, names nothing.
        Assert.Equal(harness.Cache.Constants.Continue, harness.Cache.Each(null, null));
        Assert.Equal(harness.Cache.Constants.Continue, harness.Cache.Each(Array.Empty<uint>(), null));
        var any = harness.Cache.Constants.Any;
        Assert.Equal(harness.Cache.Constants.Continue, harness.Cache.Each(new[] { any }, null));
        Assert.Equal(harness.Cache.Constants.Continue, harness.Cache.Each(new[] { any, first, any }, null));
    }

    [Fact]
    public void LinksTheStoreBelowMadeThemselvesAreReadAndNotCounted()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 5));
        var cache = harness.Cache;
        var link = harness.Make(50, 50);
        var made = UnionAndRemapLinksTests.Make(harness.Below, link, link);
        Assert.NotNull(cache.GetLink(made));
        Assert.False(cache.Expire(made, 5));

        // Deleting it reports a change the cache has no record of.
        cache.Delete(made);
        Assert.Equal(Sorted(link), harness.Addresses());
    }

    [Fact]
    public void AnAddressNamedBeforeItsLinkIsHeldFromTheStart()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 2));
        var cache = harness.Cache;
        var user = harness.Make(50, 50);
        cache.EnsureCreated(50u);
        cache.Delete(user);
        var other = harness.Make(7, 7);
        var last = harness.Make(8, 8);
        Assert.Equal(Sorted(other, last), harness.Addresses());
    }

    [Fact]
    public void ACacheOverARemapAsksItWhatItClaims()
    {
        using var stores = new Stores();
        var remap = new RemapLinksDecorator<uint>(stores.New(), new OffsetAddressMap<uint>(100));
        var cache = new CacheLinksDecorator<uint>(remap);
        Assert.False(cache.Claims(5));
        Assert.True(cache.Claims(105));
    }

    [Fact]
    public void AQueryWorksOverACache()
    {
        using var harness = new Harness(new CacheOptions(MaxLinks: 2, TimeToLive: 10));
        var cache = harness.Cache;
        AdvancedMixedQueryProcessor.ProcessQuery(cache, new AdvancedMixedQueryProcessor.Options { Query = "() ((1: 1 1) (2: 2 2))" });
        Assert.Equal(2u, Total(cache));
        AdvancedMixedQueryProcessor.ProcessQuery(cache, new AdvancedMixedQueryProcessor.Options { Query = "() ((3: 3 3))" });
        Assert.Equal(2u, Total(cache));
        harness.Now += 10;
        Assert.Equal(0u, Total(cache));
    }
}
