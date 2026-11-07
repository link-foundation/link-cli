// Edges of the small library types: writes to nothing and to many links through a
// permissions view, a union's arguments and lookups, and the value types.
using System;
using System.Collections.Generic;
using System.Linq;
using Platform.Data;
using Platform.Data.Doublets;
using Xunit;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public sealed class LibraryEdgesTests
    {
        [Fact]
        public void A_View_Refuses_A_Write_To_Many_Links_At_Once()
        {
            PermissionsDecoratorTests.WithWorld(world =>
            {
                var view = world.As(world.V["boss"]);
                var any = view.Constants.Any;
                var update = Assert.Throws<ForbiddenException>(() =>
                    view.Update(new[] { any, any, any }, new[] { 0u, 1u, 1u }, null));
                Assert.Equal("update *", update.What);
                var delete = Assert.Throws<ForbiddenException>(() => view.Delete(new[] { any, any, any }, null));
                Assert.Equal("delete *", delete.What);
            });
        }

        [Fact]
        public void A_View_Answers_A_Write_To_An_Absent_Link_As_To_A_Hidden_One()
        {
            PermissionsDecoratorTests.WithWorld(world =>
            {
                var view = world.As(world.V["alice"]);
                const uint absent = 100000u;
                var hidden = world.V["secret"];
                foreach (var address in new[] { absent, hidden })
                {
                    var target = new[] { address };
                    var update = Assert.Throws<ArgumentException>(() => { view.Update(target, new[] { address, 1u, 1u }, null); });
                    Assert.Equal($"Link not found: {address}", update.Message);
                    var delete = Assert.Throws<ArgumentException>(() => { view.Delete(target, null); });
                    Assert.Equal($"Link not found: {address}", delete.Message);
                }
                Assert.False(world.Links.Exists(absent));
                Assert.True(world.Links.Exists(hidden));
            });
        }

        [Fact]
        public void A_View_Checks_A_Short_Substitution_And_The_Store_Refuses_It()
        {
            PermissionsDecoratorTests.WithWorld(world =>
            {
                var boss = world.As(world.V["boss"]);
                var made = boss.Create();
                var before = new DoubletLink(world.Links.GetLink(made));
                var note = world.V["note"];
                Assert.ThrowsAny<Exception>(() => { boss.Update(new[] { made }, new[] { note, note }, null); });
                Assert.ThrowsAny<Exception>(() => { boss.Update(new[] { made }, Array.Empty<uint>(), null); });
                Assert.ThrowsAny<Exception>(() => { boss.Update(new[] { made }, null, null); });
                Assert.Equal(before, new DoubletLink(world.Links.GetLink(made)));
            });
        }

        [Fact]
        public void A_View_Stops_Reporting_When_The_Handler_Breaks()
        {
            PermissionsDecoratorTests.WithWorld(world =>
            {
                var boss = world.As(world.V["boss"]);
                var made = boss.Create();
                var seen = 0;
                boss.Update(new[] { made }, new[] { made, made, made }, (_, _) =>
                {
                    seen++;
                    return boss.Constants.Break;
                });
                Assert.Equal(1, seen);
            });
        }

        [Fact]
        public void A_Created_Link_Deleted_Again_Is_No_Longer_Owned()
        {
            PermissionsDecoratorTests.WithWorld(world =>
            {
                var alice = world.As(world.V["alice"]);
                var made = alice.Create();
                var row = new DoubletLink(made, 0u, 0u);
                Assert.True(alice.Owns(row));
                alice.Delete(new[] { made }, null);
                Assert.False(alice.Owns(row));
            });
        }

        [Fact]
        public void A_Name_Of_A_Link_That_May_Be_Read_But_Not_Written_Is_Forbidden()
        {
            PermissionsDecoratorTests.WithWorld(world =>
            {
                var alice = world.As(world.V["alice"]);
                var open = world.Types.Instance(world.T["Public"]);
                var forbidden = Assert.Throws<ForbiddenException>(() => alice.SetName(open, "renamed"));
                Assert.EndsWith($" {open}", forbidden.What);
            });
        }

        [Fact]
        public void A_Union_Needs_Members_That_Exist()
        {
            Assert.Throws<ArgumentException>(() => new UnionLinksDecorator<uint>());
            Assert.Throws<ArgumentException>(() => new UnionLinksDecorator<uint>(new INamedTypesLinks<uint>[] { null! }));
        }

        [Fact]
        public void A_Union_Answers_About_Addresses_And_Names_Across_Members()
        {
            using var stores = new UnionAndRemapLinksTests.Stores();
            var local = stores.New();
            var server = stores.New();
            var union = new UnionLinksDecorator<uint>(local, server);
            Assert.Equal(-1, union.OwnerOf(union.Constants.Any));
            Assert.Equal(-1, union.OwnerOf(0u));
            Assert.True(union.Claims(7u));

            var named = UnionAndRemapLinksTests.Make(server, 1u, 1u);
            server.SetName(named, "kept");
            Assert.Equal("kept", union.GetName(named));
            Assert.Null(union.GetName(99u));
            union.EnsureCreated(5u);
            Assert.Equal(0, union.OwnerOf(5u));
        }

        [Fact]
        public void Link_Parts_Are_A_Read_Only_List_Of_Three()
        {
            var row = new LinkParts<uint>(1u, 2u, 3u);
            IList<uint> list = row;
            Assert.Equal(3, list.Count);
            Assert.True(list.IsReadOnly);
            Assert.Equal(new[] { 1u, 2u, 3u }, list.ToArray());
            Assert.Equal(new[] { 1u, 2u, 3u }, ((System.Collections.IEnumerable)row).Cast<uint>().ToArray());
            Assert.Equal(1, row.IndexOf(2u));
            Assert.Equal(-1, row.IndexOf(9u));
            var holds = row.Contains(3u);
            Assert.True(holds);
            var copy = new uint[4];
            row.CopyTo(copy, 1);
            Assert.Equal(new[] { 0u, 1u, 2u, 3u }, copy);
            Assert.Throws<ArgumentOutOfRangeException>(() => row[3]);
            Assert.Throws<NotSupportedException>(() => list[0] = 1u);
            Assert.Throws<NotSupportedException>(() => list.Add(1u));
            Assert.Throws<NotSupportedException>(() => list.Insert(0, 1u));
            Assert.Throws<NotSupportedException>(() => list.Remove(1u));
            Assert.Throws<NotSupportedException>(() => list.RemoveAt(0));
            Assert.Throws<NotSupportedException>(() => list.Clear());
            Assert.True(row.Equals((object)new LinkParts<uint>(1u, 2u, 3u)));
            Assert.False(row.Equals((object)"row"));
            Assert.Equal(new LinkParts<uint>(1u, 2u, 3u).GetHashCode(), row.GetHashCode());
            Assert.Equal("(1: 2->3)", row.ToString());
            Assert.Equal("(2->3)", new LinkParts<uint>(0u, 2u, 3u).ToString());
        }

        [Fact]
        public void Errors_Keep_Their_Message_And_Cause()
        {
            var cause = new InvalidOperationException("cause");
            Exception[] plain =
            {
                new InvalidLinkFormatException("m"),
                new UnexpectedLinkStructureException("m"),
                new InvalidQueryPatternException("m"),
            };
            Assert.All(plain, error => Assert.Equal("m", error.Message));
            Exception[] caused =
            {
                new InvalidLinkFormatException("m", cause),
                new UnexpectedLinkStructureException("m", cause),
                new InvalidQueryPatternException("m", cause),
            };
            Assert.All(caused, error =>
            {
                Assert.Equal("m", error.Message);
                Assert.Same(cause, error.InnerException);
            });
            var forbidden = new ForbiddenException("update 7", cause);
            Assert.Equal("update 7", forbidden.What);
            Assert.Same(cause, forbidden.InnerException);
            Assert.Equal("Forbidden: update 7", new ForbiddenException("update 7").Message);
        }
    }
}
