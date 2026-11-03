using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;
using Xunit;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests
{
    /// <summary>
    /// Networks joined into one and networks under other addresses (issue #149):
    /// <see cref="UnionLinksDecorator{TLinkAddress}"/>,
    /// <see cref="RemapLinksDecorator{TLinkAddress}"/> and
    /// <see cref="LinksExternalIds"/>. The Rust twin is
    /// <c>rust/tests/union_and_remap_links_tests.rs</c>, case for case.
    /// </summary>
    public sealed class UnionAndRemapLinksTests
    {
        /// <summary>Stores of their own, each with its names, removed when disposed.</summary>
        internal sealed class Stores : IDisposable
        {
            private readonly List<IDisposable> _owned = new();
            private readonly List<string> _files = new();

            public NamedTypesDecorator<uint> New()
            {
                var data = Path.GetTempFileName();
                var names = Path.GetTempFileName();
                _files.Add(data);
                _files.Add(names);
                var memory = new FileMappedResizableDirectMemory(data, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                var united = new UnitedMemoryLinks<uint>(memory);
                var links = new NamedTypesDecorator<uint>(united.DecorateWithAutomaticUniquenessAndUsagesRepointing(), names);
                _owned.Add(links);
                _owned.Add(united);
                _owned.Add(memory);
                return links;
            }

            public void Dispose()
            {
                foreach (var owned in _owned)
                {
                    owned.Dispose();
                }
                foreach (var file in _files)
                {
                    File.Delete(file);
                }
            }
        }

        internal static uint Make(ILinks<uint> links, uint source, uint target)
        {
            var link = links.Create();
            links.Update(link, source, target);
            return link;
        }

        internal static List<(uint, uint, uint)> All(ILinks<uint> links, uint source = 0, uint target = 0)
        {
            var any = links.Constants.Any;
            return links.All(new DoubletLink(any, source == 0 ? any : source, target == 0 ? any : target))
                .Select(link => (link![0], link[1], link[2]))
                .OrderBy(link => link.Item1)
                .ToList();
        }

        private static void WithUnion(Action<NamedTypesDecorator<uint>, NamedTypesDecorator<uint>, UnionLinksDecorator<uint>> test)
        {
            using var stores = new Stores();
            var local = stores.New();
            var server = stores.New();
            test(local, server, new UnionLinksDecorator<uint>(local, server));
        }

        [Fact]
        public void UnionReadsTheLinksOfEveryMember()
        {
            WithUnion((local, server, union) =>
            {
                Make(server, 1u, 1u);
                Make(server, 2u, 2u);
                Make(server, 2u, 1u);
                var local1 = Make(local, 3u, 3u);
                Assert.Equal(new[] { (local1, 3u, 3u), (2u, 2u, 2u), (3u, 2u, 1u) }, All(union));
                Assert.Equal(3u, union.Count());
                Assert.True(union.Exists(3u));
                Assert.Equal(new[] { (2u, 2u, 2u), (3u, 2u, 1u) }, All(union, source: 2u));
                Assert.Equal(new[] { (local1, 3u, 3u) }, All(union, target: 3u));
                Assert.Equal(0, union.OwnerOf(1u));
                Assert.Equal(1, union.OwnerOf(2u));
                Assert.Equal(-1, union.OwnerOf(99u));
            });
        }

        [Fact]
        public void UnionFirstMemberShadowsTheRest()
        {
            WithUnion((local, server, union) =>
            {
                Make(server, 1u, 1u);
                Make(local, 2u, 2u);
                Assert.Equal(new[] { (1u, 2u, 2u) }, All(union));
                Assert.Equal(1u, union.Count());
                Assert.Empty(All(union, source: 1u));
            });
        }

        [Fact]
        public void UnionCreatesInThePrimaryAndWritesWhereTheLinkLives()
        {
            WithUnion((local, server, union) =>
            {
                Make(server, 1u, 1u);
                Make(server, 2u, 2u);
                var created = union.Create();
                union.Update(created, 2u, 2u);
                Assert.Equal(new[] { (created, 2u, 2u) }, All(local));
                union.Update(2u, 2u, created);
                Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, created) }, All(server));
                union.Delete(2u);
                Assert.Equal(new[] { (1u, 1u, 1u) }, All(server));
                Assert.Equal(new[] { (created, 2u, 2u) }, All(union));
            });
        }

        [Fact]
        public void UnionRefusesWritesToManyLinksAtOnce()
        {
            WithUnion((local, server, union) =>
            {
                var any = union.Constants.Any;
                Assert.Throws<NotSupportedException>(() => union.Delete(new DoubletLink(any, any, any), null));
                Assert.Throws<NotSupportedException>(() => union.Update(new DoubletLink(any, 1u, any), new DoubletLink(any, 2u, 2u), null));
            });
        }

        [Fact]
        public void UnionFindsNamesInMemberOrder()
        {
            WithUnion((local, server, union) =>
            {
                Make(server, 1u, 1u);
                Make(server, 2u, 2u);
                var type = Make(local, 1u, 1u);
                server.SetName(2u, "Note");
                server.SetName(1u, "Shadowed");
                Assert.Equal(2u, union.GetByName("Note"));
                Assert.Equal("Note", union.GetName(2u));
                Assert.Equal(0u, union.GetByName("Shadowed"));
                local.SetName(type, "Note");
                Assert.Equal(type, union.GetByName("Note"));
                union.SetName(2u, "Remote");
                Assert.Equal(2u, server.GetByName("Remote"));
                Assert.Equal(0u, local.GetByName("Remote"));
                union.RemoveName(2u);
                Assert.Null(server.GetName(2u));
            });
        }

        private static void WithRemap(Action<NamedTypesDecorator<uint>, RemapLinksDecorator<uint>> test)
        {
            using var stores = new Stores();
            var inner = stores.New();
            test(inner, new RemapLinksDecorator<uint>(inner, new OffsetAddressMap<uint>(1000u)));
        }

        [Fact]
        public void RemapMovesEveryAddressByAnOffset()
        {
            WithRemap((inner, remap) =>
            {
                var point = Make(inner, 1u, 1u);
                Assert.Equal(new[] { (1001u, 1001u, 1001u) }, All(remap));
                var created = remap.Create();
                Assert.Equal(1002u, created);
                remap.Update(created, 1001u, created);
                Assert.Equal(new[] { (point, point, point), (2u, 1u, 2u) }, All(inner));
                Assert.Equal(new[] { (1002u, 1001u, 1002u) }, All(remap, source: 1001u, target: 1002u));
                remap.Delete(1002u);
                Assert.Equal(new[] { (point, point, point) }, All(inner));
            });
        }

        [Fact]
        public void RemapFindsNothingAndWritesNothingOutsideItsNetwork()
        {
            WithRemap((inner, remap) =>
            {
                var point = Make(inner, 1u, 1u);
                Assert.Empty(All(remap, source: 1u));
                Assert.False(remap.Exists(1u));
                Assert.Equal(0u, remap.Count(new DoubletLink(remap.Constants.Any, 5u, remap.Constants.Any)));
                var error = Assert.Throws<ArgumentException>(() => remap.Update(1001u, 5u, 1001u));
                Assert.Contains("not of this network", error.Message, StringComparison.Ordinal);
                error = Assert.Throws<ArgumentException>(() => remap.Delete(5u));
                Assert.StartsWith("Link not found", error.Message, StringComparison.Ordinal);
                Assert.Equal(new[] { (point, point, point) }, All(inner));
            });
        }

        [Fact]
        public void RemapReportsChangesUnderItsAddresses()
        {
            WithRemap((inner, remap) =>
            {
                var changes = new List<(uint[]?, uint[]?)>();
                remap.Create(null, (before, after) =>
                {
                    changes.Add((before?.ToArray(), after?.ToArray()));
                    return remap.Constants.Continue;
                });
                remap.Update(new[] { 1001u }, new DoubletLink(1001u, 1001u, 1001u), (before, after) =>
                {
                    changes.Add((before?.ToArray(), after?.ToArray()));
                    return remap.Constants.Continue;
                });
                Assert.Equal(new uint[] { 1001u, 0u, 0u }, changes[0].Item2);
                Assert.Equal(new uint[] { 1001u, 1001u, 1001u }, changes[^1].Item2);
            });
        }

        [Fact]
        public void RemapNamesUnderItsAddresses()
        {
            WithRemap((inner, remap) =>
            {
                Make(inner, 1u, 1u);
                remap.SetName(1001u, "Type");
                Assert.Equal(1u, inner.GetByName("Type"));
                Assert.Equal(1001u, remap.GetByName("Type"));
                Assert.Equal("Type", remap.GetName(1001u));
                Assert.Null(remap.GetName(1u));
                remap.RemoveName(1001u);
                Assert.Equal(0u, remap.GetByName("Type"));
            });
        }

        [Fact]
        public void AddressTableGivesAddressesInTheOrderTheyAreSeen()
        {
            using var stores = new Stores();
            var inner = stores.New();
            var table = new AddressTable<uint>(500u);
            table.Add(900u, 2u);
            var remap = new RemapLinksDecorator<uint>(inner, table);
            Make(inner, 1u, 1u);
            Make(inner, 1u, 2u);
            Assert.Equal(new[] { (500u, 500u, 500u), (900u, 500u, 900u) }, All(remap));
            var created = remap.Create();
            Assert.Equal(501u, created);
            Assert.Equal(new[] { (500u, 1u), (501u, 3u), (900u, 2u) }, table.Pairs.Select(pair => (pair.Key, pair.Value)).OrderBy(pair => pair.Key));
            Assert.Throws<ArgumentException>(() => table.Add(500u, 7u));
            Assert.Throws<ArgumentException>(() => table.Add(600u, 1u));
        }

        [Fact]
        public void UnionOfRemappedNetworksShadowsNothing()
        {
            using var stores = new Stores();
            var local = stores.New();
            var server = stores.New();
            Make(local, 1u, 1u);
            Make(server, 1u, 1u);
            var union = new UnionLinksDecorator<uint>(local, new RemapLinksDecorator<uint>(server, new OffsetAddressMap<uint>(1u << 24)));
            var remote = (1u << 24) + 1u;
            Assert.Equal(new[] { (1u, 1u, 1u), (remote, remote, remote) }, All(union));
            var typed = Make(union, remote, 1u);
            Assert.Equal(new[] { (typed, remote, 1u) }, All(local, source: remote));
            union.Update(remote, remote, remote);
            Assert.Equal(1, union.OwnerOf(remote));
            Assert.Throws<ArgumentException>(() => union.Update(remote, remote, typed));
        }

        [Theory]
        [InlineData(false)]
        [InlineData(true)]
        public void ExternalIdsBindIdsOfOtherSystemsToLinks(bool tableApart)
        {
            using var stores = new Stores();
            var data = stores.New();
            var ids = new LinksExternalIds(tableApart ? stores.New() : data, "uuid");
            const string first = "0b4a3c1e-8f6d-4d2a-9a51-7c1f2e3d4b5a";
            const string second = "7e9d1a2b-3c4d-4e5f-8a9b-0c1d2e3f4a5b";
            Assert.Null(ids.Find(first));
            var link = ids.Ensure(first, data);
            Assert.Equal(link, ids.Ensure(first, data));
            Assert.Equal(link, ids.Find(first));
            Assert.Equal(first, ids.IdOf(link));
            Assert.Equal((link, link, link), (link, data.GetSource(link), data.GetTarget(link)));
            ids.Bind(second, link);
            Assert.Null(ids.Find(first));
            Assert.Equal(second, ids.IdOf(link));
            var other = Make(data, link, link);
            ids.Bind(second, other);
            Assert.Null(ids.IdOf(link));
            Assert.Equal(other, ids.Find(second));
            Assert.True(ids.Unbind(second));
            Assert.False(ids.Unbind(second));
            Assert.Null(ids.IdOf(other));
            Assert.Throws<ArgumentException>(() => new LinksExternalIds(data, "a:b"));
            for (var address = 1u; address <= 6u; address++)
            {
                data.EnsureCreated(address);
                ids.Bind("n" + address, address);
            }
            for (var address = 1u; address <= 6u; address++)
            {
                Assert.Equal(address, ids.Find("n" + address));
                Assert.Equal("n" + address, ids.IdOf(address));
            }
        }
    }
}
