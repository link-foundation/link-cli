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
    /// Permissions stored in links (issue #149). The cases are the "permissions"
    /// tests of <c>@link-foundation/ui-links</c> plus what a store with cascades,
    /// merges and names adds; the Rust twin is
    /// <c>rust/tests/permissions_decorator_tests.rs</c>, case for case.
    /// </summary>
    public sealed class PermissionsDecoratorTests
    {
        /// <summary>
        /// The world of ui-links' test helpers:
        /// <code>
        ///   Type -> User -> Admin              alice: User, boss: Admin
        ///   Type -> Doc -> Note -> Shared      Shared extends Public too
        ///   Type -> Doc -> Secret
        ///   Type -> Public
        /// </code>
        /// Admins write anything, users write notes and read what is public.
        /// </summary>
        internal sealed class World
        {
            public required NamedTypesDecorator<uint> Links { get; init; }
            public required TypesDecorator<uint> Types { get; init; }
            public required PermissionMarkers<uint> Markers { get; init; }
            public required ILinks<uint> Rules { get; init; }
            public required PermissionPolicy<uint> Policy { get; init; }
            public required Dictionary<string, uint> T { get; init; }
            public required Dictionary<string, uint> V { get; init; }

            public PermissionsDecorator<uint> As(uint subject) => new(Links, Policy, subject);

            public List<DoubletLink> All(ILinks<uint> links) =>
                links.All(new DoubletLink(links.Constants.Any, links.Constants.Any, links.Constants.Any))
                    .Select(link => new DoubletLink(link))
                    .OrderBy(link => link.Index)
                    .ToList();
        }

        /// <summary>Runs <paramref name="test"/> in a new world, its rules in the data or in a store of their own.</summary>
        internal static void WithWorld(Action<World> test, bool separateRules = false)
        {
            var files = Enumerable.Range(0, 4).Select(_ => Path.GetTempFileName()).ToList();
            try
            {
                using var memory = new FileMappedResizableDirectMemory(files[0], UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var united = new UnitedMemoryLinks<uint>(memory);
                using var links = new NamedTypesDecorator<uint>(united.DecorateWithAutomaticUniquenessAndUsagesRepointing(), files[1]);
                using var rulesMemory = new FileMappedResizableDirectMemory(files[2], UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var rulesUnited = new UnitedMemoryLinks<uint>(rulesMemory);
                using var apart = new NamedTypesDecorator<uint>(rulesUnited.DecorateWithAutomaticUniquenessAndUsagesRepointing(), files[3]);
                test(MakeWorld(links, separateRules ? apart : links));
            }
            finally
            {
                foreach (var file in files)
                {
                    File.Delete(file);
                }
            }
        }

        /// <summary>The world of ui-links' test helpers in <paramref name="links"/>, its rules in <paramref name="rules"/>.</summary>
        internal static World MakeWorld(NamedTypesDecorator<uint> links, ILinks<uint> rules)
        {
            var types = new TypesDecorator<uint>(links);
            var markers = PermissionPolicy<uint>.EnsureMarkers(links, links);
            var t = new Dictionary<string, uint>();
            void Define(string name, params uint[] parents)
            {
                t[name] = parents.Length == 0 ? types.Define() : types.Define(parents[0], parents.Skip(1).ToArray());
                links.SetName(t[name], name);
            }
            Define("User");
            Define("Admin", t["User"]);
            Define("Doc");
            Define("Public");
            Define("Note", t["Doc"]);
            Define("Shared", t["Note"], t["Public"]);
            Define("Secret", t["Doc"]);
            var v = new Dictionary<string, uint>();
            foreach (var (name, type) in new[] { ("alice", "User"), ("boss", "Admin"), ("note", "Note"), ("shared", "Shared"), ("secret", "Secret") })
            {
                v[name] = types.Instance(t[type]);
                links.SetName(v[name], name);
            }
            PermissionPolicy<uint>.Allow(rules, markers, PermissionAccess.Write, t["Admin"], markers.Anything);
            PermissionPolicy<uint>.Allow(rules, markers, PermissionAccess.Write, t["User"], t["Note"]);
            PermissionPolicy<uint>.Allow(rules, markers, PermissionAccess.Read, t["User"], t["Public"]);
            return new World
            {
                Links = links,
                Types = types,
                Markers = markers,
                Rules = rules,
                Policy = new PermissionPolicy<uint>(rules, markers, types),
                T = t,
                V = v,
            };
        }

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
            catch (ArgumentException)
            {
                return "bad-request";
            }
        }

        [Fact]
        public void Implements_INamedTypesLinks()
        {
            WithWorld(w =>
            {
                var view = w.As(w.V["alice"]);
                Assert.IsAssignableFrom<ILinks<uint>>(view);
                Assert.IsAssignableFrom<INamedTypesLinks<uint>>(view);
                Assert.False(typeof(PermissionsDecorator<uint>).IsSealed);
                Assert.False(typeof(PermissionPolicy<uint>).IsSealed);
            });
        }

        [Fact]
        public void Markers_Are_Named_Points()
        {
            WithWorld(w =>
            {
                var m = w.Markers;
                foreach (var (marker, name) in new[] { (m.AllowRead, "AllowRead"), (m.AllowWrite, "AllowWrite"), (m.Anything, "Anything"), (m.AllowReference, "AllowReference") })
                {
                    Assert.Equal(new DoubletLink(marker, marker, marker), new DoubletLink(w.Links.GetLink(marker)));
                    Assert.Equal(marker, w.Links.GetByName(name));
                }
                Assert.Equal(m, PermissionPolicy<uint>.EnsureMarkers(w.Links, w.Links));
            });
        }

        [Theory]
        [InlineData(false)]
        [InlineData(true)]
        public void Answers_By_Type(bool separateRules)
        {
            WithWorld(w =>
            {
                IList<uint> Link(uint id) => w.Links.GetLink(id)!;
                var (alice, boss, note, shared, secret) = (w.V["alice"], w.V["boss"], w.V["note"], w.V["shared"], w.V["secret"]);
                var policy = w.Policy;
                Assert.True(policy.CanWrite(alice, Link(note)));
                Assert.True(policy.CanRead(alice, Link(shared)));
                Assert.False(policy.CanRead(alice, Link(secret)));
                Assert.True(policy.CanWrite(boss, Link(secret)));
                Assert.False(policy.CanRead(0, Link(note)));
                Assert.True(policy.CanCreate(alice));
                Assert.False(policy.CanCreate(0));
                if (separateRules)
                {
                    // Three rules, each a pair and its mark.
                    Assert.Equal(6u, w.Rules.Count());
                }
            }, separateRules);
        }

        [Theory]
        [InlineData(false)]
        [InlineData(true)]
        public void Shows_A_Subject_Only_What_It_May_Read(bool separateRules)
        {
            WithWorld(w =>
            {
                var view = w.As(w.V["alice"]);
                // The sub-type Shared is of type Note; its Extends pair is not.
                Assert.Equal(new[] { w.T["Shared"], w.V["note"], w.V["shared"] }, w.All(view).Select(link => link.Index));
                Assert.Equal(3u, view.Count());
                Assert.False(view.Exists(w.V["secret"]));
                Assert.True(view.Exists(w.V["note"]));
                Assert.Equal("not-found", ErrorOf(() => view.Delete(w.V["secret"])));
                Assert.Equal("not-found", ErrorOf(() => view.Update(w.V["secret"], w.T["Note"], w.V["secret"])));
            }, separateRules);
        }

        [Theory]
        [InlineData(false)]
        [InlineData(true)]
        public void Refuses_Writes_The_Store_Unchanged(bool separateRules)
        {
            WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var view = w.As(v["alice"]);
                var before = w.All(w.Links);
                Assert.Equal("forbidden", ErrorOf(() => view.Update(v["note"], t["Secret"], v["note"])));
                Assert.Equal("forbidden", ErrorOf(() => view.Update(v["shared"], t["Public"], v["shared"])));
                view.Update(v["shared"], t["Note"], v["shared"]);
                view.Update(v["shared"], t["Shared"], v["shared"]);
                Assert.Equal(before, w.All(w.Links));
                var id = view.Create();
                view.Update(id, 0u, id);
                view.Update(id, t["Note"], id);
                Assert.Equal("forbidden", ErrorOf(() => view.Update(id, t["Secret"], id)));
                view.Delete(id);
                Assert.Equal(before, w.All(w.Links));
                var guest = w.As(0);
                Assert.Equal("forbidden", ErrorOf(() => guest.Create()));
                Assert.Equal(before, w.All(w.Links));
            }, separateRules);
        }

        [Fact]
        public void Keeps_The_Hierarchy_From_Who_May_Write_Its_Values()
        {
            WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                PermissionPolicy<uint>.Allow(w.Rules, w.Markers, PermissionAccess.Reference, t["User"], t["Admin"]);
                var view = w.As(v["alice"]);
                var pair = w.Links.SearchOrDefault(t["Shared"], t["Public"]);
                Assert.NotEqual(0u, pair);
                // The link of Note itself is of type Doc.
                Assert.Equal("not-found", ErrorOf(() => view.Update(t["Note"], t["Admin"], t["Note"])));
                // The pair Shared -> Public is of type Extends.
                Assert.Equal("not-found", ErrorOf(() => view.Update(pair, t["Shared"], t["Admin"])));
                var more = view.Create();
                view.Update(more, t["Shared"], t["Admin"]);
                var mark = view.Create();
                Assert.Equal("forbidden", ErrorOf(() => view.Update(mark, w.Types.Extends, more)));
                var boss = w.As(v["boss"]);
                boss.Update(pair, t["Shared"], t["Public"]);
            });
        }

        [Fact]
        public void Refuses_A_Marker_As_A_Subject()
        {
            WithWorld(w =>
            {
                var m = w.Markers;
                Assert.Equal("bad-request", ErrorOf(() => PermissionPolicy<uint>.Allow(w.Rules, m, PermissionAccess.Read, m.AllowRead, 1)));
                Assert.Equal("bad-request", ErrorOf(() => PermissionPolicy<uint>.Allow(w.Rules, m, PermissionAccess.Write, m.AllowWrite, 1)));
                Assert.Equal("bad-request", ErrorOf(() => PermissionPolicy<uint>.Allow(w.Rules, m, PermissionAccess.Reference, m.AllowReference, 1)));
                Assert.Equal(PermissionAccess.Reference, PermissionPolicy<uint>.ParseAccess("reference"));
                Assert.Equal("bad-request", ErrorOf(() => PermissionPolicy<uint>.ParseAccess("own")));
                Assert.Throws<ArgumentOutOfRangeException>(() => PermissionPolicy<uint>.Allow(w.Rules, m, (PermissionAccess)7, 1, 1));
            });
        }

        [Theory]
        [InlineData(false)]
        [InlineData(true)]
        public void Gives_A_Rule_Given_Twice_Once(bool separateRules)
        {
            WithWorld(w =>
            {
                var (t, m) = (w.T, w.Markers);
                var count = w.Rules.Count();
                var rule = w.Rules.SearchOrDefault(t["User"], t["Note"]);
                Assert.NotEqual(0u, rule);
                Assert.Equal(rule, PermissionPolicy<uint>.Allow(w.Rules, m, PermissionAccess.Write, t["User"], t["Note"]));
                Assert.Equal(count, w.Rules.Count());
                Assert.True(w.Policy.CanWrite(w.V["alice"], w.Links.GetLink(w.V["note"])!));
                // The same pair may carry both marks.
                Assert.Equal(rule, PermissionPolicy<uint>.Allow(w.Rules, m, PermissionAccess.Read, t["User"], t["Note"]));
                Assert.Equal(count + 1, w.Rules.Count());
            }, separateRules);
        }

        [Fact]
        public void Owns_What_It_Created_While_Untyped()
        {
            WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var view = w.As(v["alice"]);
                var id = view.Create();
                Assert.True(view.Exists(id));
                Assert.True(view.Owns(w.Links.GetLink(id)!));
                // Another subject does not own it, nor does a new view.
                Assert.False(w.As(v["alice"]).Exists(id));
                view.Update(id, t["Note"], id);
                Assert.False(view.Owns(w.Links.GetLink(id)!));
                // Undoing the update that typed it makes it its own again.
                w.Links.Update(id, 0u, 0u);
                Assert.True(view.Owns(w.Links.GetLink(id)!));
                // A self-reference has a type, itself, so the policy judges it (#152: only
                // a store read as triplets has points of its own).
                Assert.False(view.Owns(new Link<uint>(id, id, id)));
                view.Forget(id);
                Assert.False(view.Exists(id));
                Assert.Equal("forbidden", ErrorOf(() => w.As(0).Create()));
            });
        }

        [Fact]
        public void Refuses_A_Delete_That_Would_Take_A_Hidden_Link_With_It()
        {
            WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                // The secret refers to alice's note: deleting the note deletes it.
                var boss = w.As(v["boss"]);
                var secretUse = boss.Create();
                boss.Update(secretUse, t["Secret"], v["note"]);
                w.Links.SetName(secretUse, "use");
                var before = w.All(w.Links);
                var view = w.As(v["alice"]);
                Assert.Equal("forbidden", ErrorOf(() => view.Delete(v["note"])));
                Assert.Equal(before, w.All(w.Links));
                Assert.Equal("note", w.Links.GetName(v["note"]));
                Assert.Equal("use", w.Links.GetName(secretUse));
                // Boss may delete both.
                boss.Delete(v["note"]);
                Assert.False(w.Links.Exists(secretUse));
                Assert.False(w.Links.Exists(v["note"]));
            });
        }

        [Fact]
        public void Refuses_A_Merge_That_Would_Repoint_A_Hidden_Link()
        {
            WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var boss = w.As(v["boss"]);
                // Two equal notes but for their targets; the secret refers to one.
                var other = boss.Create();
                boss.Update(other, t["Note"], v["shared"]);
                var secretUse = boss.Create();
                boss.Update(secretUse, t["Secret"], other);
                var before = w.All(w.Links);
                var view = w.As(v["alice"]);
                // Making `other` equal to the note merges it into the note,
                // repointing the secret.
                Assert.Equal(new DoubletLink(v["note"], t["Note"], v["note"]), new DoubletLink(w.Links.GetLink(v["note"])));
                Assert.Equal("forbidden", ErrorOf(() => view.Update(other, t["Note"], v["note"])));
                Assert.Equal(before, w.All(w.Links));
                // Boss may.
                boss.Update(other, t["Note"], v["note"]);
                Assert.False(w.Links.Exists(other));
                Assert.Equal(new DoubletLink(secretUse, t["Secret"], v["note"]), new DoubletLink(w.Links.GetLink(secretUse)));
            });
        }

        [Fact]
        public void Reports_Every_Change_Of_A_Write()
        {
            WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var view = w.As(v["alice"]);
                var id = view.Create();
                var changes = new List<(DoubletLink? Before, DoubletLink? After)>();
                view.Update(new LinkAddress<uint>(id), new DoubletLink(id, t["Note"], id), (before, after) =>
                {
                    changes.Add((before is null ? (DoubletLink?)null : new DoubletLink(before), after is null ? (DoubletLink?)null : new DoubletLink(after)));
                    return view.Constants.Continue;
                });
                Assert.Equal(new (DoubletLink?, DoubletLink?)[] { (new DoubletLink(id, 0, 0), new DoubletLink(id, t["Note"], id)) }, changes);
            });
        }

        [Fact]
        public void Names_Need_The_Right_To_Write()
        {
            WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var view = w.As(v["alice"]);
                // Names are references: any name resolves.
                // Names follow references: the name of a hidden link is an unknown name.
                Assert.Equal(0u, view.GetByName("secret"));
                Assert.Equal("Note", view.GetName(t["Note"]));
                view.SetName(v["note"], "my note");
                Assert.Equal("my note", w.Links.GetName(v["note"]));
                Assert.Equal("not-found", ErrorOf(() => view.SetName(v["secret"], "x")));
                Assert.Equal("not-found", ErrorOf(() => w.As(0).SetName(v["note"], "y")));
                // Taking the name of a link it may not write is taking a write.
                Assert.Equal("forbidden", ErrorOf(() => view.SetName(v["note"], "secret")));
                Assert.Equal(v["secret"], w.Links.GetByName("secret"));
                view.RemoveName(v["secret"]);
                Assert.Equal("secret", w.Links.GetName(v["secret"]));
                view.RemoveName(v["note"]);
                Assert.Null(w.Links.GetName(v["note"]));
            });
        }
    }
}
