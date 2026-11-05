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
    /// The type hierarchy of issue #148. The cases are the "types" tests of
    /// <c>@link-foundation/ui-links</c> plus depth, diamonds and cycles; the
    /// Rust twin is <c>rust/tests/types_decorator_tests.rs</c>, case for case.
    /// </summary>
    public sealed class TypesDecoratorTests
    {
        /// <summary>
        /// A world of types and values, the one of ui-links' test helpers:
        /// <code>
        ///   Type -> User -> Admin              alice: User, boss: Admin
        ///   Type -> Doc -> Note -> Shared      Shared extends Public too
        ///   Type -> Doc -> Secret
        ///   Type -> Public
        /// </code>
        /// </summary>
        private sealed class World
        {
            public required TypesDecorator<uint> Types { get; init; }
            public required NamedTypesDecorator<uint> Names { get; init; }
            public Dictionary<string, uint> T { get; } = new();
            public Dictionary<string, uint> V { get; } = new();

            public void Define(string name, params uint[] parents)
            {
                T[name] = parents.Length == 0 ? Types.Define() : Types.Define(parents[0], parents.Skip(1).ToArray());
                Names.SetName(T[name], name);
            }

            public void Instance(string name, uint type)
            {
                V[name] = Types.Instance(type);
                Names.SetName(V[name], name);
            }
        }

        private static void WithNamedLinks(Action<NamedTypesDecorator<uint>> test)
        {
            var dataFile = Path.GetTempFileName();
            var namesFile = Path.GetTempFileName();
            try
            {
                using var memory = new FileMappedResizableDirectMemory(dataFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var unitedLinks = new UnitedMemoryLinks<uint>(memory);
                using var named = new NamedTypesDecorator<uint>(unitedLinks.DecorateWithAutomaticUniquenessAndUsagesRepointing(), namesFile);
                test(named);
            }
            finally
            {
                File.Delete(dataFile);
                File.Delete(namesFile);
            }
        }

        private static void WithTypes(Action<TypesDecorator<uint>> test)
        {
            WithNamedLinks(named => test(new TypesDecorator<uint>(named)));
        }

        private static void WithWorld(Action<World> test)
        {
            WithNamedLinks(named =>
            {
                var world = new World { Types = new TypesDecorator<uint>(named), Names = named };
                world.Define("User");
                world.Define("Admin", world.T["User"]);
                world.Define("Doc");
                world.Define("Public");
                world.Define("Note", world.T["Doc"]);
                world.Define("Shared", world.T["Note"], world.T["Public"]);
                world.Define("Secret", world.T["Doc"]);
                world.Instance("alice", world.T["User"]);
                world.Instance("boss", world.T["Admin"]);
                world.Instance("note", world.T["Note"]);
                world.Instance("shared", world.T["Shared"]);
                world.Instance("secret", world.T["Secret"]);
                test(world);
            });
        }

        [Fact]
        public void Implements_ILinks_And_ITypes()
        {
            WithTypes(types =>
            {
                Assert.IsAssignableFrom<ILinks<uint>>(types);
                Assert.IsAssignableFrom<ITypes<uint>>(types);
                Assert.False(typeof(TypesDecorator<uint>).IsSealed);
            });
        }

        [Fact]
        public void Markers_Are_Named_Points()
        {
            WithNamedLinks(named =>
            {
                var types = new TypesDecorator<uint>(named);
                Assert.Equal(new DoubletLink(types.Root, types.Root, types.Root), new DoubletLink(named.GetLink(types.Root)));
                Assert.Equal(new DoubletLink(types.Extends, types.Extends, types.Extends), new DoubletLink(named.GetLink(types.Extends)));
                Assert.Equal(types.Root, named.GetByName("Type"));
                Assert.Equal(types.Extends, named.GetByName("Extends"));
                // Markers that exist are found again, not created twice.
                var again = new TypesDecorator<uint>(named);
                Assert.Equal((types.Root, types.Extends), (again.Root, again.Extends));
            });
        }

        [Fact]
        public void Follows_SubTypes_And_Multiple_Inheritance()
        {
            WithWorld(w =>
            {
                var types = w.Types;
                Assert.True(types.IsA(w.V["shared"], w.T["Doc"]));
                Assert.True(types.IsA(w.V["shared"], w.T["Public"]));
                Assert.False(types.IsA(w.V["note"], w.T["Public"]));
                Assert.True(types.IsA(w.V["boss"], w.T["User"]));
                Assert.True(types.IsA(w.T["Shared"], w.T["Shared"]));
                Assert.False(types.IsOf(w.T["Shared"], w.T["Shared"]));
                Assert.Equal(w.T["Secret"], types.TypeOf(w.V["secret"]));
                Assert.True(types.IsType(w.T["Note"]));
                Assert.False(types.IsType(9999));
                Assert.Equal(new[] { w.T["Note"], w.T["Public"] }, types.ParentsOf(w.T["Shared"]));
                Assert.Equal(
                    new[] { w.T["Shared"], w.T["Note"], w.T["Public"], w.T["Doc"], types.Root },
                    types.AncestorsOf(w.T["Shared"]));
            });
        }

        [Fact]
        public void Checks_A_Link_That_Is_Not_Stored_Yet()
        {
            WithWorld(w =>
            {
                Assert.True(w.Types.IsA(new DoubletLink(0, w.T["Note"], 0), w.T["Doc"]));
                Assert.False(w.Types.IsA(new DoubletLink(0, w.T["Note"], 0), w.T["Public"]));
            });
        }

        [Fact]
        public void Needs_An_Extends_Marker_For_More_Parents()
        {
            WithNamedLinks(named =>
            {
                var root = TypesDecorator<uint>.EnsurePoint(named, named, "Type");
                var types = new TypesDecorator<uint>(named, root);
                Assert.Equal(0u, types.Extends);
                Assert.Throws<InvalidOperationException>(() => types.Define(root, root));
                Assert.Throws<InvalidOperationException>(() => types.Extend(root, root));
                var single = types.Define(root);
                Assert.Equal(new uint[] { root }, types.ParentsOf(single));
            });
        }

        [Fact]
        public void Acceptance_Builds_The_Hierarchy_As_Links()
        {
            WithTypes(types =>
            {
                var type = types.Root;
                var user = types.Define(type);
                var admin = types.Define(user);
                var doc = types.Define(type);
                var @public = types.Define(type);
                var note = types.Define(doc);
                var news = types.Define(note, doc, @public);

                // Each definition is a link: (S: parent S), the extra parents
                // are pairs marked by Extends.
                Assert.Equal(new DoubletLink(user, type, user), new DoubletLink(types.GetLink(user)));
                Assert.Equal(new DoubletLink(admin, user, admin), new DoubletLink(types.GetLink(admin)));
                Assert.Equal(new DoubletLink(news, note, news), new DoubletLink(types.GetLink(news)));
                foreach (var parent in new[] { doc, @public })
                {
                    var pair = types.SearchOrDefault(news, parent);
                    Assert.NotEqual(0u, pair);
                    Assert.NotEqual(0u, types.SearchOrDefault(types.Extends, pair));
                    Assert.True(types.IsExtension(pair));
                }

                Assert.True(types.IsA(news, doc));
                Assert.True(types.IsA(news, @public));
                Assert.True(types.IsA(news, note));
                Assert.True(types.IsA(news, type));
                Assert.False(types.IsA(news, user));
                Assert.True(types.IsA(admin, user));
                Assert.Equal(new[] { note, doc, @public }, types.ParentsOf(news));

                // A value of Note that also extends Public.
                var item = types.Instance(news);
                Assert.True(types.IsA(item, doc));
                Assert.True(types.IsA(item, @public));
                Assert.Equal(news, types.TypeOf(item));
            });
        }

        [Fact]
        public void Follows_Depth_Three_And_More()
        {
            WithTypes(types =>
            {
                var a = types.Define();
                var b = types.Define(a);
                var c = types.Define(b);
                var d = types.Define(c);
                var e = types.Define(d);
                var value = types.Instance(e);
                foreach (var type in new[] { e, d, c, b, a, types.Root })
                {
                    Assert.True(types.IsA(value, type));
                    Assert.True(types.IsOf(value, type));
                }
                Assert.False(types.IsA(a, b));
                Assert.Equal(new[] { e, d, c, b, a, types.Root }, types.AncestorsOf(e));
            });
        }

        [Fact]
        public void Visits_A_Diamond_Once()
        {
            WithTypes(types =>
            {
                var top = types.Define();
                var left = types.Define(top);
                var right = types.Define(top);
                var bottom = types.Define(left, right);
                var value = types.Instance(bottom);
                Assert.True(types.IsA(value, top));
                Assert.True(types.IsA(value, left));
                Assert.True(types.IsA(value, right));
                Assert.Equal(new[] { bottom, left, right, top, types.Root }, types.AncestorsOf(bottom));
            });
        }

        [Fact]
        public void Terminates_On_An_Extends_Cycle()
        {
            WithTypes(types =>
            {
                // A extends B and B extends A.
                var a = types.Define();
                var b = types.Define();
                types.Extend(a, b);
                types.Extend(b, a);
                var unrelated = types.Define();
                var valueOfA = types.Instance(a);
                var valueOfB = types.Instance(b);

                Assert.Equal(new[] { types.Root, b }, types.ParentsOf(a));
                Assert.Equal(new[] { types.Root, a }, types.ParentsOf(b));
                Assert.True(types.IsA(valueOfA, b));
                Assert.True(types.IsA(valueOfB, a));
                Assert.True(types.IsOf(a, b));
                Assert.True(types.IsOf(b, a));
                Assert.False(types.IsA(valueOfA, unrelated));
                // Through the cycle a type is strictly of itself.
                Assert.True(types.IsOf(a, a));
                Assert.Equal(new[] { a, types.Root, b }, types.AncestorsOf(a));
                Assert.Equal(new[] { b, types.Root, a }, types.AncestorsOf(b));
            });
        }

        [Fact]
        public void Refuses_To_Extend_A_Direct_SubType()
        {
            WithTypes(types =>
            {
                var a = types.Define();
                var b = types.Define(a);
                // (b: a b) is the doublet (a b) itself.
                Assert.Throws<InvalidOperationException>(() => types.Extend(a, b));
                Assert.False(types.IsExtension(b));
                Assert.Equal(new[] { a }, types.ParentsOf(b));
                Assert.Equal(new[] { types.Root }, types.ParentsOf(a));
            });
        }

        [Fact]
        public void Terminates_On_A_Source_Cycle()
        {
            WithTypes(types =>
            {
                var a = types.Define();
                var b = types.Define(a);
                // (a: b a) and (b: a b): each is a sub-type of the other.
                types.Update(a, b, a);
                var unrelated = types.Define();

                Assert.Equal(new[] { b }, types.ParentsOf(a));
                Assert.Equal(new[] { a }, types.ParentsOf(b));
                Assert.True(types.IsOf(a, b));
                Assert.False(types.IsOf(a, types.Root));
                Assert.False(types.IsA(types.Instance(a), unrelated));
                Assert.Equal(new[] { a, b }, types.AncestorsOf(a));
            });
        }

        [Fact]
        public void Wiring_Links_Are_Of_Type_Extends()
        {
            WithWorld(w =>
            {
                var types = w.Types;
                var pair = types.SearchOrDefault(w.T["Shared"], w.T["Public"]);
                var mark = types.SearchOrDefault(types.Extends, pair);
                Assert.True(types.IsExtension(pair));
                Assert.False(types.IsExtension(mark));
                Assert.False(types.IsExtension(w.T["Shared"]));
                Assert.True(types.IsA(pair, types.Extends));
                Assert.True(types.IsA(mark, types.Extends));
                Assert.False(types.IsA(pair, w.T["Shared"]));
                Assert.False(types.IsA(pair, w.T["Doc"]));
                Assert.Equal(types.Extends, types.TypeOf(mark));
            });
        }

        [Fact]
        public void Instances_With_A_Target()
        {
            WithWorld(w =>
            {
                var types = w.Types;
                var membership = types.Instance(w.T["User"], w.V["boss"]);
                Assert.Equal(new DoubletLink(membership, w.T["User"], w.V["boss"]), new DoubletLink(types.GetLink(membership)));
                Assert.Equal(w.T["User"], types.TypeOf(membership));
                Assert.False(types.IsType(membership));
                Assert.True(types.IsA(membership, w.T["User"]));
                Assert.True(types.IsA(membership, types.Root));
                // The same doublet is the same value.
                Assert.Equal(membership, types.Instance(w.T["User"], w.V["boss"]));
            });
        }

        [Fact]
        public void Answers_For_Missing_Links()
        {
            WithWorld(w =>
            {
                var types = w.Types;
                Assert.Equal(0u, types.TypeOf(9999));
                Assert.False(types.IsType(9999));
                Assert.Empty(types.ParentsOf(9999));
                Assert.Equal(new uint[] { 9999 }, types.AncestorsOf(9999));
                Assert.False(types.IsA(9999, 9999));
                Assert.False(types.IsOf(9999, types.Root));
                Assert.Throws<ArgumentException>(() => types.Define(9999));
                Assert.Throws<ArgumentException>(() => types.Define(types.Root, 9999));
                Assert.Throws<ArgumentException>(() => types.Instance(9999));
            });
        }

        [Fact]
        public void Keeps_The_Hierarchy_In_A_Named_File_Store()
        {
            var dataFile = Path.GetTempFileName();
            var namesFile = Path.GetTempFileName();
            try
            {
                uint note, news, @public;
                using (var memory = new FileMappedResizableDirectMemory(dataFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep))
                using (var unitedLinks = new UnitedMemoryLinks<uint>(memory))
                using (var named = new NamedTypesDecorator<uint>(unitedLinks.DecorateWithAutomaticUniquenessAndUsagesRepointing(), namesFile))
                {
                    var types = new TypesDecorator<uint>(named);
                    var doc = types.Define();
                    @public = types.Define();
                    note = types.Define(doc);
                    news = types.Define(note, @public);
                    named.SetName(@public, "Public");
                }
                using (var memory = new FileMappedResizableDirectMemory(dataFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep))
                using (var unitedLinks = new UnitedMemoryLinks<uint>(memory))
                using (var named = new NamedTypesDecorator<uint>(unitedLinks.DecorateWithAutomaticUniquenessAndUsagesRepointing(), namesFile))
                {
                    var types = new TypesDecorator<uint>(named);
                    Assert.Equal(@public, named.GetByName("Public"));
                    Assert.Equal(new[] { note, @public }, types.ParentsOf(news));
                    Assert.True(types.IsA(types.Instance(news), @public));
                }
            }
            finally
            {
                File.Delete(dataFile);
                File.Delete(namesFile);
            }
        }

        [Fact]
        public void Pinned_Types_Are_SubTypes_Of_The_Root()
        {
            var dataFile = Path.GetTempFileName();
            try
            {
                using var memory = new FileMappedResizableDirectMemory(dataFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var unitedLinks = new UnitedMemoryLinks<uint>(memory);
                var pinned = new PinnedTypesDecorator<uint>(unitedLinks.DecorateWithAutomaticUniquenessAndUsagesRepointing());
                var (type, unicodeSymbol, unicodeSequence) = pinned;
                Assert.Equal((1u, 2u, 3u), (type, unicodeSymbol, unicodeSequence));

                var extends = pinned.Create();
                pinned.Update(extends, extends, extends);
                var types = new TypesDecorator<uint>(pinned, type, extends);

                Assert.True(types.IsType(type));
                Assert.Empty(types.ParentsOf(type));
                foreach (var pinnedType in new[] { unicodeSymbol, unicodeSequence })
                {
                    Assert.True(types.IsType(pinnedType));
                    Assert.Equal(type, types.TypeOf(pinnedType));
                    Assert.Equal(new[] { type }, types.ParentsOf(pinnedType));
                    Assert.True(types.IsOf(pinnedType, type));
                }
                var text = types.Define(unicodeSequence, unicodeSymbol);
                Assert.True(types.IsA(types.Instance(text), type));
                Assert.Equal(new[] { text, unicodeSequence, unicodeSymbol, type }, types.AncestorsOf(text));
                // The pinned types still enumerate as before.
                Assert.Equal(new uint[] { 1, 2, 3 }, pinned.Take(3).ToArray());
            }
            finally
            {
                File.Delete(dataFile);
            }
        }
    }
}
