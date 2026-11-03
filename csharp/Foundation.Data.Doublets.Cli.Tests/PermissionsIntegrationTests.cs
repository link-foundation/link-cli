using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Platform.Data;
using Platform.Data.Doublets;
using Xunit;
using static Foundation.Data.Doublets.Cli.Tests.UnionAndRemapLinksTests;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests
{
    /// <summary>
    /// The permission layer with the other layers (issue #149): over a union,
    /// a remap, transactions and version control, under them, and with roots
    /// and type systems other than the named <c>Type</c> point. The world is
    /// the one of <see cref="PermissionsDecoratorTests"/>; the Rust twin is
    /// <c>rust/tests/permissions_integration_tests.rs</c>, case for case.
    /// </summary>
    public sealed class PermissionsIntegrationTests
    {
        /// <summary>Where a server's network reads in a union with a client's.</summary>
        private const uint ServerOffset = 1u << 24;

        private static uint[] Indices(ILinks<uint> links) => All(links).Select(link => link.Item1).ToArray();

        private static string? ErrorOf(Action action)
        {
            try
            {
                action();
                return null;
            }
            catch (ForbiddenException)
            {
                return "forbidden";
            }
            catch (ArgumentException error) when (error.Message.StartsWith("Link not found", StringComparison.Ordinal))
            {
                return "not-found";
            }
        }

        private static uint Typed(ILinks<uint> links, uint type)
        {
            var link = links.Create();
            links.Update(link, type, link);
            return link;
        }

        [Fact]
        public void PermissionsOverAUnionRestrictEveryMember()
        {
            using var stores = new Stores();
            var client = stores.New();
            var server = stores.New();
            var w = PermissionsDecoratorTests.MakeWorld(server, server);
            uint S(uint address) => address + ServerOffset;
            var union = new UnionLinksDecorator<uint>(client, new RemapLinksDecorator<uint>(server, new OffsetAddressMap<uint>(ServerOffset)));
            // Markers, types and rules are found through the union, in whichever member they are.
            var policy = new PermissionPolicy<uint>(union, union);
            Assert.Equal(S(w.Markers.AllowRead), policy.Markers.AllowRead);
            var alice = new PermissionsDecorator<uint>(union, policy, S(w.V["alice"]));
            var boss = new PermissionsDecorator<uint>(union, policy, S(w.V["boss"]));
            Assert.Equal(new[] { S(w.T["Shared"]), S(w.V["note"]), S(w.V["shared"]) }, Indices(alice));

            // Links of the client, typed by types of the server.
            var draft = Typed(alice, S(w.T["Note"]));
            var hidden = Typed(boss, S(w.T["Secret"]));
            Assert.Equal(new[] { (draft, S(w.T["Note"]), draft), (hidden, S(w.T["Secret"]), hidden) }, All(client));
            Assert.Equal(new[] { draft, S(w.T["Shared"]), S(w.V["note"]), S(w.V["shared"]) }, Indices(alice));
            Assert.Equal("not-found", ErrorOf(() => alice.Delete(hidden)));
            Assert.Equal("not-found", ErrorOf(() => alice.Delete(S(w.V["secret"]))));
            Assert.Equal("forbidden", ErrorOf(() => alice.Update(draft, S(w.T["Secret"]), draft)));
            Assert.Equal("forbidden", ErrorOf(() => Typed(new PermissionsDecorator<uint>(union, policy, 0u), S(w.T["Note"]))));

            // A rule of the client counts as much as the rules of the server.
            PermissionPolicy<uint>.Allow(boss, policy.Markers, PermissionAccess.Read, S(w.T["User"]), S(w.T["Secret"]));
            Assert.Contains(hidden, Indices(alice));
            Assert.Contains(S(w.V["secret"]), Indices(alice));
            Assert.Equal(new[] { (S(w.T["User"]), S(w.T["Secret"])) }, policy.Granted(policy.Markers.AllowRead).Where(rule => rule.Object == S(w.T["Secret"])));
        }

        [Fact]
        public void ARefusedDeleteOverAUnionComesBackInItsMember()
        {
            using var stores = new Stores();
            var client = stores.New();
            var server = stores.New();
            var w = PermissionsDecoratorTests.MakeWorld(server, server);
            uint S(uint address) => address + ServerOffset;
            var union = new UnionLinksDecorator<uint>(client, new RemapLinksDecorator<uint>(server, new OffsetAddressMap<uint>(ServerOffset)));
            var policy = new PermissionPolicy<uint>(union, union);
            // A hidden link of the server refers to alice's note: deleting the note deletes it.
            var boss = new PermissionsDecorator<uint>(server, new PermissionPolicy<uint>(server, server), w.V["boss"]);
            var hiddenUse = boss.Create();
            boss.Update(hiddenUse, w.T["Secret"], w.V["note"]);
            var before = All(server);
            var alice = new PermissionsDecorator<uint>(union, policy, S(w.V["alice"]));
            Assert.Equal("forbidden", ErrorOf(() => alice.Delete(S(w.V["note"]))));
            // Both come back in the server, at their addresses, and the client gets nothing.
            Assert.Equal(before, All(server));
            Assert.Empty(All(client));
            Assert.Equal("note", union.GetName(S(w.V["note"])));
            Assert.Equal(1, union.OwnerOf(S(hiddenUse)));
        }

        [Fact]
        public void PermissionsUnderAUnionRestrictTheirMemberOnly()
        {
            using var stores = new Stores();
            var client = stores.New();
            var server = stores.New();
            var w = PermissionsDecoratorTests.MakeWorld(server, server);
            uint S(uint address) => address + ServerOffset;
            var remote = new RemapLinksDecorator<uint>(server, new OffsetAddressMap<uint>(ServerOffset));
            var seen = new PermissionsDecorator<uint>(remote, new PermissionPolicy<uint>(remote, remote), S(w.V["alice"]));
            var union = new UnionLinksDecorator<uint>(client, seen);
            var own = Make(client, 1u, 1u);
            Assert.Equal(new[] { own, S(w.T["Shared"]), S(w.V["note"]), S(w.V["shared"]) }, Indices(union));
            Assert.False(union.Exists(S(w.V["secret"])));
            Assert.Equal(-1, union.OwnerOf(S(w.V["secret"])));
            Assert.Equal("forbidden", ErrorOf(() => union.Update(S(w.V["note"]), S(w.T["Secret"]), S(w.V["note"]))));
            union.Update(own, S(w.T["Secret"]), own);
            Assert.Equal(new[] { (own, S(w.T["Secret"]), own) }, All(client));
        }

        [Fact]
        public void ARemapOverPermissionsSeesWhatTheSubjectMay()
        {
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var remap = new RemapLinksDecorator<uint>(w.As(w.V["alice"]), new OffsetAddressMap<uint>(1000u));
                Assert.Equal(new[] { w.T["Shared"], w.V["note"], w.V["shared"] }.Select(index => index + 1000u), Indices(remap));
                Assert.Equal("forbidden", ErrorOf(() => remap.Update(w.V["note"] + 1000u, w.T["Secret"] + 1000u, w.V["note"] + 1000u)));
                Assert.Equal("not-found", ErrorOf(() => remap.Delete(w.V["secret"] + 1000u)));
                Assert.Equal(w.V["note"] + 1000u, remap.GetByName("note"));
            }, separateRules: true);
        }

        [Fact]
        public void PermissionsOverTransactionsLogOnlyWhatTheyAllow()
        {
            PermissionsDecoratorTests.WithWorld(w =>
            {
                using var log = new Stores();
                using var transactions = new TransactionsDecorator(w.Links, log.New());
                var alice = new PermissionsDecorator<uint>(transactions, w.Policy, w.V["alice"]);
                Assert.Equal(new[] { w.T["Shared"], w.V["note"], w.V["shared"] }, Indices(alice));
                var logged = transactions.Log.Count;
                Assert.Equal("forbidden", ErrorOf(() => alice.Update(w.V["note"], w.T["Secret"], w.V["note"])));
                Assert.Equal("not-found", ErrorOf(() => alice.Delete(w.V["secret"])));
                Assert.Equal(logged, transactions.Log.Count);
                var draft = Typed(alice, w.T["Note"]);
                Assert.True(transactions.Log.Count > logged);
                Assert.Equal(new DoubletLink(draft, w.T["Note"], draft), new DoubletLink(w.Links.GetLink(draft)));
            });
        }

        [Fact]
        public void TransactionsOverPermissionsLogOnlyWhatTheyAllow()
        {
            PermissionsDecoratorTests.WithWorld(w =>
            {
                using var log = new Stores();
                using var transactions = new TransactionsDecorator(w.As(w.V["alice"]), log.New());
                Assert.Equal(new[] { w.T["Shared"], w.V["note"], w.V["shared"] }, Indices(transactions));
                Assert.Equal("forbidden", ErrorOf(() => transactions.Update(w.V["note"], w.T["Secret"], w.V["note"])));
                Assert.Empty(transactions.Log);
                Typed(transactions, w.T["Note"]);
                Assert.NotEmpty(transactions.Log);
            });
        }

        [Fact]
        public void PermissionsOverVersionControlSeeEveryBranchThroughTheRules()
        {
            PermissionsDecoratorTests.WithWorld(w =>
            {
                using var stores = new Stores();
                using var transactions = new TransactionsDecorator(w.Links, stores.New());
                using var versions = new VersionControlDecorator(transactions, stores.New());
                var alice = new PermissionsDecorator<uint>(versions, w.Policy, w.V["alice"]);
                Assert.Equal(new[] { w.T["Shared"], w.V["note"], w.V["shared"] }, Indices(alice));
                Assert.Equal("forbidden", ErrorOf(() => alice.Update(w.V["note"], w.T["Secret"], w.V["note"])));
                var draft = Typed(alice, w.T["Note"]);
                Assert.Contains(draft, Indices(alice));
                Assert.Equal(new DoubletLink(draft, w.T["Note"], draft), new DoubletLink(w.Links.GetLink(draft)));
            });
        }

        [Fact]
        public void RootsAndMarkersMayBeUnnamedLinksOfAnyAddress()
        {
            using var stores = new Stores();
            var data = stores.New();
            var rules = stores.New();
            // A root of its own, not the point named Type: any link may be one.
            var root = Make(data, 1u, 1u);
            var types = new TypesDecorator<uint>(data, root);
            var markers = new PermissionMarkers<uint>(Make(data, 2u, 2u), Make(data, 3u, 3u), Make(data, 4u, 4u));
            var user = types.Define();
            var note = types.Define();
            var alice = types.Instance(user);
            var mine = types.Instance(note);
            var other = Typed(data, root);
            PermissionPolicy<uint>.Allow(rules, markers, PermissionAccess.Write, user, note);
            var view = new PermissionsDecorator<uint>(data, new PermissionPolicy<uint>(rules, markers, types), alice);
            Assert.Equal(new[] { mine }, Indices(view));
            Assert.Equal("not-found", ErrorOf(() => view.Delete(other)));
            Assert.Equal(0u, data.GetByName("Type"));
            Assert.Equal(0u, data.GetByName("AllowWrite"));
        }

        /// <summary>A type system of its own: a link is of the type that is its source, and nothing extends anything.</summary>
        private sealed class SourceTypedPolicy : PermissionPolicy<uint>
        {
            private readonly ILinks<uint> _data;

            public SourceTypedPolicy(ILinks<uint> data, ILinks<uint> rules, PermissionMarkers<uint> markers)
                : base(rules, markers, new TypesDecorator<uint>(data, data.Constants.Null))
            {
                _data = data;
            }

            protected override bool IsSubject(uint subject, uint type) =>
                type == Markers.Anything || (subject != 0 && (subject == type || (_data.Exists(subject) && _data.GetSource(subject) == type)));

            protected override bool IsObject(IList<uint> link, uint type) =>
                type == Markers.Anything || link[_data.Constants.SourcePart] == type;
        }

        [Fact]
        public void APolicyMayBringATypeSystemOfItsOwn()
        {
            using var stores = new Stores();
            var data = stores.New();
            var markers = PermissionPolicy<uint>.EnsureMarkers(data, data);
            var person = Make(data, 0u, 0u);
            var post = Make(data, 0u, 0u);
            data.Update(person, person, person);
            data.Update(post, post, post);
            var alice = Make(data, person, 0u);
            var mine = Make(data, post, alice);
            var other = Make(data, person, person);
            PermissionPolicy<uint>.Allow(data, markers, PermissionAccess.Write, person, post);
            var view = new PermissionsDecorator<uint>(data, new SourceTypedPolicy(data, data, markers), alice);
            // The type of posts is a post too: it starts at itself.
            Assert.Equal(new[] { post, mine }, Indices(view));
            Assert.Equal("not-found", ErrorOf(() => view.Delete(other)));
            var draft = Make(view, post, person);
            Assert.Equal("forbidden", ErrorOf(() => view.Update(draft, person, post)));
        }

        /// <summary>A policy of no rules at all: everyone reads everything, a link is written by the subject it starts at.</summary>
        private sealed class OwnerPolicy : IPermissionPolicy<uint>
        {
            public PermissionMarkers<uint> Markers => default;

            public bool CanRead(uint subject, IList<uint> link) => subject != 0;

            public bool CanWrite(uint subject, IList<uint> link) => subject != 0 && link[1] == subject;

            public bool CanCreate(uint subject) => subject != 0;
        }

        [Fact]
        public void APolicyMayKeepNoRulesAtAll()
        {
            using var stores = new Stores();
            var data = stores.New();
            var alice = Make(data, 1u, 1u);
            var bob = Make(data, 2u, 2u);
            var post = Make(data, bob, alice);
            var asAlice = new PermissionsDecorator<uint>(data, new OwnerPolicy(), alice);
            Assert.Equal(new[] { alice, bob, post }, Indices(asAlice));
            Assert.Equal("forbidden", ErrorOf(() => asAlice.Delete(post)));
            var mine = Make(asAlice, alice, bob);
            Assert.Equal(new DoubletLink(mine, alice, bob), new DoubletLink(data.GetLink(mine)));
            Assert.Equal("forbidden", ErrorOf(() => asAlice.Update(mine, bob, bob)));
            Assert.Equal("forbidden", ErrorOf(() => Make(new PermissionsDecorator<uint>(data, new OwnerPolicy(), 0u), 1u, 1u)));
            Assert.Empty(All(new PermissionsDecorator<uint>(data, new OwnerPolicy(), 0u)));
        }
    }
}
