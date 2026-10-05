using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests
{
    /// <summary>
    /// An update into an existing doublet merges into it, and every link that used the merged-away address
    /// is re-pointed at the surviving one (https://github.com/linksplatform/Data.Doublets/issues/515).
    /// </summary>
    public sealed class LinksUniquenessAndUsagesRepointingResolverTests : IDisposable
    {
        private readonly string _databaseFile = Path.GetTempFileName();
        private readonly UnitedMemoryLinks<uint> _store;
        private readonly ILinks<uint> _links;

        public LinksUniquenessAndUsagesRepointingResolverTests()
        {
            _store = new UnitedMemoryLinks<uint>(_databaseFile);
            _links = _store.DecorateWithAutomaticUniquenessAndUsagesRepointing();
        }

        public void Dispose()
        {
            _store.Dispose();
            File.Delete(_databaseFile);
        }

        private uint Store(uint source, uint target) => _links.CreateAndUpdate(source, target);

        private uint Point() => _links.CreatePoint();

        private void Update(uint index, uint source, uint target) =>
            _links.Update(new DoubletLink(index, _links.Constants.Any, _links.Constants.Any), new DoubletLink(index, source, target), null);

        private List<DoubletLink> AllLinks() =>
            _links.All(new DoubletLink(_links.Constants.Any, _links.Constants.Any, _links.Constants.Any))
                .Select(link => new DoubletLink(link))
                .ToList();

        [Fact]
        public void RepointUsagesReplacesOnlyTheHalfThatNamedTheMergedLink()
        {
            var merged = Point();
            var surviving = Point();
            var unrelated = Point();
            var usageAsSource = Store(merged, unrelated);
            var usageAsTarget = Store(unrelated, merged);
            var resolver = Assert.IsType<LinksUniquenessAndUsagesRepointingResolver<uint>>(_links);

            resolver.RepointUsages(merged, surviving, null);

            Assert.Equal(
                new[]
                {
                    new DoubletLink(merged, merged, merged),
                    new DoubletLink(surviving, surviving, surviving),
                    new DoubletLink(unrelated, unrelated, unrelated),
                    new DoubletLink(usageAsSource, surviving, unrelated),
                    new DoubletLink(usageAsTarget, unrelated, surviving),
                },
                AllLinks());
        }

        [Fact]
        public void AnUpdateIntoAnExistingDoubletRepointsTheUsagesOfTheMergedLink()
        {
            var surviving = Store(0, 0);
            var merged = Point();
            Update(surviving, surviving, merged);

            Update(merged, surviving, merged);

            Assert.Equal(new[] { new DoubletLink(surviving, surviving, surviving) }, AllLinks());
        }

        [Fact]
        public void AnUpdateReportsTheRepointedUsageAndTheDeletedLink()
        {
            var surviving = Store(0, 0);
            var merged = Point();
            Update(surviving, surviving, merged);
            var changes = new List<(DoubletLink Before, DoubletLink After)>();

            _links.Update(
                new DoubletLink(merged, _links.Constants.Any, _links.Constants.Any),
                new DoubletLink(merged, surviving, merged),
                (before, after) =>
                {
                    changes.Add((new DoubletLink(before), new DoubletLink(after)));
                    return _links.Constants.Continue;
                });

            Assert.Contains((new DoubletLink(surviving, surviving, merged), new DoubletLink(surviving, surviving, surviving)), changes);
            Assert.Equal(new DoubletLink(0, 0, 0), changes[^1].After);
            Assert.Equal(merged, changes[^1].Before.Index);
        }

        [Fact]
        public void AUsageOfTheMergedLinkInBothHalvesIsRepointedInOneUpdate()
        {
            var surviving = Point();
            var other = Point();
            var merged = Store(surviving, other);
            var usage = Store(merged, merged);

            Update(merged, surviving, surviving);

            // The usage becomes (surviving surviving), which is the surviving link itself, so it merges in too.
            Assert.Equal(
                new[] { new DoubletLink(surviving, surviving, surviving), new DoubletLink(other, other, other) },
                AllLinks());
            Assert.False(_links.Exists(usage));
        }

        [Fact]
        public void AUsageMergedAwayWhileAnEarlierOneIsRepointedIsSkipped()
        {
            var surviving = Point();
            var merged = Point();
            var usageAsSource = Store(merged, surviving);
            var usageAsTarget = Store(surviving, merged);
            // Re-pointing usageAsSource makes it the surviving link, so this one becomes (surviving merged),
            // a duplicate of usageAsTarget, and merges into it before the loop reaches it.
            var usageOfAUsage = Store(usageAsSource, merged);

            Update(merged, surviving, surviving);

            Assert.Equal(new[] { new DoubletLink(surviving, surviving, surviving) }, AllLinks());
            Assert.False(_links.Exists(usageAsTarget));
            Assert.False(_links.Exists(usageOfAUsage));
        }
    }
}
