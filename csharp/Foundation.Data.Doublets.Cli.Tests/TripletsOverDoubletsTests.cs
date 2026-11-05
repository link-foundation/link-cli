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
    /// Triplets stored in doublets (issue #151). The Rust twin is
    /// <c>rust/tests/triplets_over_doublets_tests.rs</c>, case for case, with the
    /// same pseudo-random sequence in the parity test.
    /// </summary>
    public sealed class TripletsOverDoubletsTests
    {
        /// <summary>A file-backed named store with triplets over it, deleted on dispose.</summary>
        private sealed class Store : IDisposable
        {
            private readonly string _dataFile = Path.GetTempFileName();
            private readonly string _namesFile = Path.GetTempFileName();
            private readonly FileMappedResizableDirectMemory _memory;
            private readonly UnitedMemoryLinks<uint> _united;

            public Store()
            {
                _memory = new FileMappedResizableDirectMemory(_dataFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                _united = new UnitedMemoryLinks<uint>(_memory);
                Named = new NamedTypesDecorator<uint>(_united.DecorateWithAutomaticUniquenessAndUsagesRepointing(), _namesFile);
                Triplets = new TripletsOverDoublets<uint>(Named);
            }

            public NamedTypesDecorator<uint> Named { get; }

            public TripletsOverDoublets<uint> Triplets { get; set; }

            public void Dispose()
            {
                Named.Dispose();
                _united.Dispose();
                _memory.Dispose();
                File.Delete(_dataFile);
                File.Delete(_namesFile);
            }
        }

        private static Triplet<uint> T(uint index, uint type, uint from, uint to) => new(index, type, from, to);

        private static TripletQuery<uint> Q(uint? index, uint? type, uint? from, uint? to) => new(index, type, from, to);

        private static DoubletLink Doublet(ILinks<uint> links, uint id) => new(links.GetLink(id));

        private static List<DoubletLink> AllLinks(ILinks<uint> links)
        {
            var all = new List<DoubletLink>();
            links.Each(null, link =>
            {
                all.Add(new DoubletLink(link!));
                return links.Constants.Continue;
            });
            return all;
        }

        /// <summary>Every doublet is a marker, a triplet, or a wrapper or pair still in use.</summary>
        private static void AssertNoOrphans(ITriplets<uint> triplets, bool allowForeign)
        {
            foreach (var link in AllLinks(triplets))
            {
                var role = triplets.RoleOf(link.Index);
                Assert.NotNull(role);
                if (role!.Value.IsAuxiliary())
                {
                    Assert.True(triplets.DoubletUsages(link.Index) > 0, $"orphan {role} {link}");
                }
                else if (role == DoubletRole.Foreign)
                {
                    Assert.True(allowForeign, $"a doublet that is no triplet: {link}");
                }
            }
        }

        /// <summary><c>Type</c>, <c>Person</c>, <c>knows</c>, <c>alice</c>, <c>bob</c> and <c>carol</c>.</summary>
        private sealed record People(uint Root, uint Person, uint Knows, uint Alice, uint Bob, uint Carol);

        private static People CreatePeople(ITriplets<uint> triplets)
        {
            var root = triplets.CreateTripletPoint();
            var person = triplets.CreateTriplet(root, 0, 0);
            var knows = triplets.CreateTriplet(root, 0, 0);
            var alice = triplets.CreateTriplet(person, 0, 0);
            var bob = triplets.CreateTriplet(person, 0, 0);
            var carol = triplets.CreateTriplet(person, 0, 0);
            return new People(root, person, knows, alice, bob, carol);
        }

        [Fact]
        public void Implements_Named_Type_Links_And_Triplets()
        {
            using var store = new Store();
            Assert.IsAssignableFrom<INamedTypesLinks<uint>>(store.Triplets);
            Assert.IsAssignableFrom<ITriplets<uint>>(store.Triplets);
        }

        [Fact]
        public void Markers_Are_Named_Points()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var markers = triplets.Markers;
            Assert.Equal(markers.Pair, triplets.GetByName(TripletsOverDoublets<uint>.PairMarkerName));
            Assert.Equal(markers.Nothing, triplets.GetByName(TripletsOverDoublets<uint>.NothingMarkerName));
            foreach (var marker in new[] { markers.Pair, markers.Nothing })
            {
                Assert.Equal(new DoubletLink(marker, marker, marker), Doublet(triplets, marker));
                Assert.Equal(DoubletRole.Marker, triplets.RoleOf(marker));
                Assert.False(triplets.IsTriplet(marker));
            }
            // The same markers are found again.
            var again = new TripletsOverDoublets<uint>(store.Named);
            Assert.Equal(markers, again.Markers);
        }

        [Fact]
        public void Stores_Nodes_And_Links()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var markers = triplets.Markers;
            var p = CreatePeople(triplets);
            // A node (i: T 0 0) is the doublet (i: T i), a value of #148.
            Assert.Equal(new DoubletLink(p.Root, p.Root, p.Root), Doublet(triplets, p.Root));
            Assert.Equal(new DoubletLink(p.Alice, p.Person, p.Alice), Doublet(triplets, p.Alice));
            Assert.Equal(T(p.Alice, p.Person, 0, 0), triplets.GetTriplet(p.Alice));
            Assert.Equal(T(p.Root, p.Root, 0, 0), triplets.GetTriplet(p.Root));

            var link = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            Assert.Equal(T(link, p.Knows, p.Alice, p.Bob), triplets.GetTriplet(link));
            // (link: knows pair), pair = (wrapper bob), wrapper = (Pair alice).
            var outer = Doublet(triplets, link);
            Assert.Equal(p.Knows, outer.Source);
            var pair = Doublet(triplets, outer.Target);
            Assert.Equal(p.Bob, pair.Target);
            Assert.Equal(DoubletRole.Pair, triplets.RoleOf(pair.Index));
            var wrapper = Doublet(triplets, pair.Source);
            Assert.Equal(new DoubletLink(wrapper.Index, markers.Pair, p.Alice), wrapper);
            Assert.Equal(DoubletRole.Wrapper, triplets.RoleOf(wrapper.Index));
            Assert.Null(triplets.RoleOf(10_000));
            Assert.Null(triplets.GetTriplet(wrapper.Index));
            Assert.Null(triplets.GetTriplet(0));
            Assert.Equal(11, AllLinks(triplets).Count);
            AssertNoOrphans(triplets, false);
        }

        [Fact]
        public void Zero_Parts_Are_Nothing()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var markers = triplets.Markers;
            var p = CreatePeople(triplets);
            var fromOnly = triplets.CreateTriplet(p.Knows, p.Alice, 0);
            var toOnly = triplets.CreateTriplet(p.Knows, 0, p.Bob);
            Assert.Equal(T(fromOnly, p.Knows, p.Alice, 0), triplets.GetTriplet(fromOnly));
            Assert.Equal(T(toOnly, p.Knows, 0, p.Bob), triplets.GetTriplet(toOnly));
            var outer = Doublet(triplets, toOnly);
            var pair = Doublet(triplets, outer.Target);
            var wrapper = Doublet(triplets, pair.Source);
            Assert.Equal(markers.Nothing, wrapper.Target);
            Assert.Equal(new[] { T(toOnly, p.Knows, 0, p.Bob) }, triplets.EachTriplet(Q(null, p.Knows, 0, null)));
            Assert.Equal(new[] { T(fromOnly, p.Knows, p.Alice, 0) }, triplets.EachTriplet(Q(null, null, p.Alice, 0)));
            // Nodes have neither: an empty from finds them as well.
            Assert.Equal(7, triplets.CountTriplets(TripletQuery<uint>.ByFrom(0)));
            Assert.Equal(7, triplets.CountTriplets(TripletQuery<uint>.ByTo(0)));
            Assert.Equal(6, triplets.CountTriplets(Q(null, null, 0, 0)));
            AssertNoOrphans(triplets, false);
        }

        [Fact]
        public void Duplicates_Get_Their_Own_Address()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var p = CreatePeople(triplets);
            var doublets = AllLinks(triplets).Count;
            var first = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            var second = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            var third = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            Assert.NotEqual(first, second);
            Assert.NotEqual(second, third);
            // The second and third go through duplicate pairs (pair i).
            var pair = Doublet(triplets, first).Target;
            var outer = Doublet(triplets, second);
            var duplicate = Doublet(triplets, outer.Target);
            Assert.Equal(new DoubletLink(duplicate.Index, pair, second), duplicate);
            Assert.Equal(DoubletRole.DuplicatePair, triplets.RoleOf(duplicate.Index));
            var expected = new[]
            {
                T(first, p.Knows, p.Alice, p.Bob),
                T(second, p.Knows, p.Alice, p.Bob),
                T(third, p.Knows, p.Alice, p.Bob),
            };
            Assert.Equal(expected, triplets.EachTriplet(TripletQuery<uint>.ByFrom(p.Alice)));
            Assert.Equal(expected, triplets.EachTriplet(TripletQuery<uint>.ByTo(p.Bob)));
            Assert.Equal(expected, triplets.EachTriplet(Q(null, p.Knows, p.Alice, p.Bob)));
            Assert.Equal(3, triplets.CountTriplets(Q(null, null, null, p.Bob)));

            triplets.DeleteTriplet(first);
            Assert.Equal(expected[1..], triplets.EachTriplet(TripletQuery<uint>.ByFrom(p.Alice)));
            AssertNoOrphans(triplets, false);
            triplets.DeleteTriplet(third);
            triplets.DeleteTriplet(second);
            Assert.Equal(doublets, AllLinks(triplets).Count);
            AssertNoOrphans(triplets, false);
        }

        [Fact]
        public void Update_Keeps_The_Address_And_Collects_The_Old_Pair()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var p = CreatePeople(triplets);
            var doublets = AllLinks(triplets).Count;
            var link = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            var oldPair = Doublet(triplets, link).Target;
            var before = triplets.UpdateTriplet(link, p.Knows, p.Alice, p.Carol);
            Assert.Equal(T(link, p.Knows, p.Alice, p.Bob), before);
            Assert.Equal(T(link, p.Knows, p.Alice, p.Carol), triplets.GetTriplet(link));
            Assert.False(triplets.Exists(oldPair), "the old pair is collected");
            Assert.Equal(doublets + 3, AllLinks(triplets).Count);

            // A new type and new parts, then a node and back: the address stays.
            triplets.UpdateTriplet(link, p.Person, p.Bob, p.Alice);
            Assert.Equal(T(link, p.Person, p.Bob, p.Alice), triplets.GetTriplet(link));
            triplets.UpdateTriplet(link, p.Person, 0, 0);
            Assert.Equal(T(link, p.Person, 0, 0), triplets.GetTriplet(link));
            Assert.Equal(doublets + 1, AllLinks(triplets).Count);
            triplets.UpdateTriplet(link, link, link, link);
            Assert.Equal(T(link, link, link, link), triplets.GetTriplet(link));
            AssertNoOrphans(triplets, false);
            triplets.UpdateTriplet(link, p.Knows, p.Alice, p.Bob);
            Assert.Equal(doublets + 3, AllLinks(triplets).Count);
            AssertNoOrphans(triplets, false);
            var unchanged = triplets.UpdateTriplet(link, p.Knows, p.Alice, p.Bob);
            Assert.Equal(T(link, p.Knows, p.Alice, p.Bob), unchanged);
            Assert.Equal(p.Root, triplets.GetTriplet(p.Person)!.Value.Type);
        }

        [Fact]
        public void A_Shared_Pair_Is_Kept_While_It_Is_Used()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var p = CreatePeople(triplets);
            var knows = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            var met = triplets.CreateTriplet(p.Person, p.Alice, p.Bob);
            var pair = Doublet(triplets, knows).Target;
            Assert.Equal(pair, Doublet(triplets, met).Target);
            triplets.UpdateTriplet(knows, p.Knows, p.Alice, p.Carol);
            Assert.True(triplets.Exists(pair), "the pair is still used");
            triplets.DeleteTriplet(met);
            Assert.False(triplets.Exists(pair));
            AssertNoOrphans(triplets, false);
            // An update into a slot another triplet holds takes a duplicate pair, and
            // an update out of it collects the duplicate pair.
            var first = triplets.CreateTriplet(p.Knows, p.Bob, p.Carol);
            var second = triplets.CreateTriplet(p.Person, p.Bob, p.Carol);
            triplets.UpdateTriplet(second, p.Knows, p.Bob, p.Carol);
            var outer = Doublet(triplets, second);
            Assert.Equal(DoubletRole.DuplicatePair, triplets.RoleOf(outer.Target));
            Assert.Equal(3, triplets.CountTriplets(TripletQuery<uint>.ByTo(p.Carol)));
            triplets.UpdateTriplet(second, p.Person, p.Bob, p.Carol);
            Assert.Equal(Doublet(triplets, first).Target, Doublet(triplets, second).Target);
            AssertNoOrphans(triplets, false);
        }

        [Fact]
        public void Delete_Cascades_To_The_Triplets_That_Use_It()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var p = CreatePeople(triplets);
            var markersAndNodes = AllLinks(triplets).Count;
            var ab = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            var bc = triplets.CreateTriplet(p.Knows, p.Bob, p.Carol);
            var about = triplets.CreateTriplet(p.Knows, ab, bc);
            var deleted = triplets.DeleteTriplet(p.Alice);
            Assert.Equal(new[] { p.Alice, ab, about }, deleted.Select(triplet => triplet.Index));
            Assert.True(triplets.IsTriplet(bc));
            AssertNoOrphans(triplets, false);
            // A cycle: two triplets that use each other.
            var first = triplets.CreateTriplet(p.Knows, p.Bob, 0);
            var second = triplets.CreateTriplet(p.Knows, first, 0);
            triplets.UpdateTriplet(first, p.Knows, second, 0);
            triplets.DeleteTriplet(second);
            Assert.False(triplets.Exists(first));
            triplets.DeleteTriplet(bc);
            Assert.Equal(markersAndNodes - 1, AllLinks(triplets).Count);
            AssertNoOrphans(triplets, false);
            // The root takes every triplet with it.
            triplets.DeleteTriplet(p.Root);
            Assert.Equal(2, AllLinks(triplets).Count);
        }

        [Fact]
        public void Refuses_Parts_That_Are_Not_Triplets()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var markers = triplets.Markers;
            var p = CreatePeople(triplets);
            var link = triplets.CreateTriplet(p.Knows, p.Alice, p.Bob);
            var pair = Doublet(triplets, link).Target;
            Assert.ThrowsAny<Exception>(() => triplets.CreateTriplet(0, 0, 0));
            Assert.ThrowsAny<Exception>(() => triplets.CreateTriplet(markers.Pair, 0, 0));
            Assert.ThrowsAny<Exception>(() => triplets.CreateTriplet(pair, 0, 0));
            Assert.ThrowsAny<Exception>(() => triplets.CreateTriplet(p.Knows, 10_000, 0));
            Assert.ThrowsAny<Exception>(() => triplets.CreateTriplet(p.Knows, 0, markers.Nothing));
            Assert.ThrowsAny<Exception>(() => triplets.UpdateTriplet(pair, p.Knows, 0, 0));
            Assert.ThrowsAny<Exception>(() => triplets.UpdateTriplet(link, 0, 0, 0));
            Assert.ThrowsAny<Exception>(() => triplets.DeleteTriplet(pair));
            Assert.ThrowsAny<Exception>(() => triplets.DeleteTriplet(10_000));
            Assert.Equal(T(link, p.Knows, p.Alice, p.Bob), triplets.GetTriplet(link));
        }

        /// <summary>
        /// The type hierarchy of #148 reads triplet nodes as values of their types,
        /// and its <c>Extends</c> wiring is not mistaken for triplets.
        /// </summary>
        [Fact]
        public void Works_With_The_Type_Hierarchy()
        {
            using var store = new Store();
            var types = new TypesDecorator<uint>(store.Named);
            var triplets = new TripletsOverDoublets<uint>(types, store.Named);
            var root = types.Root;
            Assert.Equal(T(root, root, 0, 0), triplets.GetTriplet(root));
            var person = types.Define();
            var @public = types.Define();
            var admin = types.Define(person, @public);
            var boss = triplets.CreateTriplet(admin, 0, 0);
            Assert.True(types.IsA(boss, person));
            Assert.True(types.IsA(boss, @public));
            Assert.Equal(
                new[] { T(person, root, 0, 0), T(@public, root, 0, 0) },
                triplets.EachTriplet(TripletQuery<uint>.ByType(root)).Where(triplet => triplet.Index != root && triplet.Index > @public - 2));
            var knows = triplets.CreateTriplet(root, 0, 0);
            var link = triplets.CreateTriplet(knows, boss, admin);
            Assert.Equal(new[] { T(link, knows, boss, admin) }, triplets.EachTriplet(TripletQuery<uint>.ByTo(admin)));
            AssertNoOrphans(triplets, true);
        }

        /// <summary>The same pseudo-random sequence in C# and Rust: a 64-bit LCG.</summary>
        private sealed class Random
        {
            private ulong _state;

            public Random(ulong seed) => _state = seed;

            public int Next(int bound)
            {
                unchecked
                {
                    _state = _state * 6_364_136_223_846_793_005UL + 1_442_695_040_888_963_407UL;
                }
                return (int)((_state >> 33) % (ulong)bound);
            }
        }

        /// <summary>A triplet store kept in a map: what the decorator must agree with.</summary>
        private sealed class ReferenceStore
        {
            public SortedDictionary<uint, (uint Type, uint From, uint To)> Map { get; } = new();

            public List<Triplet<uint>> Each(TripletQuery<uint> query)
            {
                return Map.Select(entry => T(entry.Key, entry.Value.Type, entry.Value.From, entry.Value.To)).Where(query.Matches).ToList();
            }

            public void Delete(uint index)
            {
                var pending = new Stack<uint>();
                pending.Push(index);
                while (pending.Count > 0)
                {
                    var deleted = pending.Pop();
                    if (!Map.Remove(deleted))
                    {
                        continue;
                    }
                    foreach (var (user, (type, from, to)) in Map)
                    {
                        if (type == deleted || from == deleted || to == deleted)
                        {
                            pending.Push(user);
                        }
                    }
                }
            }
        }

        private static uint Part(Random random, IReadOnlyList<uint> ids)
        {
            return random.Next(4) == 0 ? 0 : ids[random.Next(ids.Count)];
        }

        [Fact]
        public void Each_Agrees_With_A_Triplet_Store_On_A_Thousand_Triplets()
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var reference = new ReferenceStore();
            var random = new Random(151);
            var root = triplets.CreateTripletPoint();
            reference.Map[root] = (root, 0, 0);
            var @protected = new HashSet<uint> { root };
            for (var i = 0; i < 10; i++)
            {
                var type = triplets.CreateTriplet(root, 0, 0);
                reference.Map[type] = (root, 0, 0);
                @protected.Add(type);
            }
            var (creates, updates, deletes) = (0, 0, 0);
            for (var step = 0; ; step++)
            {
                if (step >= 1600 && reference.Map.Count >= 1000)
                {
                    break;
                }
                var ids = reference.Map.Keys.ToList();
                var choice = step >= 1600 ? 0 : random.Next(10);
                if (choice <= 6)
                {
                    var type = ids[random.Next(ids.Count)];
                    var from = Part(random, ids);
                    var to = Part(random, ids);
                    var index = triplets.CreateTriplet(type, from, to);
                    Assert.True(reference.Map.TryAdd(index, (type, from, to)));
                    creates++;
                }
                else if (choice <= 8)
                {
                    var index = ids[random.Next(ids.Count)];
                    if (@protected.Contains(index))
                    {
                        continue;
                    }
                    var type = ids[random.Next(ids.Count)];
                    var from = Part(random, ids);
                    var to = Part(random, ids);
                    triplets.UpdateTriplet(index, type, from, to);
                    reference.Map[index] = (type, from, to);
                    updates++;
                }
                else
                {
                    var index = ids[random.Next(ids.Count)];
                    if (!@protected.Contains(index))
                    {
                        var deleted = triplets.DeleteTriplet(index);
                        var before = reference.Map.Count;
                        reference.Delete(index);
                        Assert.Equal(before - reference.Map.Count, deleted.Count);
                        deletes++;
                    }
                }
            }
            Assert.True(creates > 1000 && updates > 100 && deletes > 50, $"{creates} {updates} {deletes}");
            Assert.True(reference.Map.Count >= 1000);

            Assert.Equal(reference.Each(TripletQuery<uint>.Any), triplets.EachTriplet(TripletQuery<uint>.Any));
            var all = reference.Map.Keys.ToList();
            foreach (var id in all.Append(0u))
            {
                foreach (var query in new[]
                {
                    TripletQuery<uint>.ByType(id),
                    TripletQuery<uint>.ByFrom(id),
                    TripletQuery<uint>.ByTo(id),
                    TripletQuery<uint>.ByIndex(id),
                })
                {
                    Assert.Equal(reference.Each(query), triplets.EachTriplet(query));
                    Assert.Equal(reference.Each(query).Count, triplets.CountTriplets(query));
                }
            }
            for (var i = 0; i < 200; i++)
            {
                var type = Part(random, all);
                var from = Part(random, all);
                var to = Part(random, all);
                var query = Q(null, type, from, to);
                Assert.Equal(reference.Each(query), triplets.EachTriplet(query));
                var picked = reference.Map.ElementAt(random.Next(all.Count)).Value;
                foreach (var combined in new[]
                {
                    Q(null, picked.Type, picked.From, null),
                    Q(null, picked.Type, null, picked.To),
                    Q(null, null, picked.From, picked.To),
                    Q(null, picked.Type, picked.From, picked.To),
                })
                {
                    Assert.Equal(reference.Each(combined), triplets.EachTriplet(combined));
                }
            }
            AssertNoOrphans(triplets, false);

            while (reference.Map.Count > 0)
            {
                var index = reference.Map.Keys.First();
                triplets.DeleteTriplet(index);
                reference.Delete(index);
                Assert.Equal(reference.Map.Count, triplets.CountTriplets(TripletQuery<uint>.Any));
            }
            Assert.Equal(2, AllLinks(triplets).Count);
        }

        private const string People_ = "(Type: Type 0 0)\n(Person: Type 0 0)\n(knows: Type 0 0)\n(alice: Person 0 0)\n(bob: Person 0 0)\n(carol: Person 0 0)\n(r: knows alice bob)\n";

        private static Store NamedPeople()
        {
            var store = new Store();
            TripletsLino.Import(store.Triplets, People_);
            return store;
        }

        private static uint Id(ITriplets<uint> triplets, string name)
        {
            var id = triplets.GetByName(name);
            Assert.NotEqual(0u, id);
            return id;
        }

        private static Triplet<uint> Named(ITriplets<uint> triplets, string index, string type, string from, string to)
        {
            return T(Id(triplets, index), Id(triplets, type), from == "0" ? 0 : Id(triplets, from), to == "0" ? 0 : Id(triplets, to));
        }

        [Fact]
        public void Lino_Round_Trip_Keeps_Names_And_Addresses()
        {
            using (var store = NamedPeople())
            {
                var triplets = store.Triplets;
                Assert.Equal(7, triplets.CountTriplets(TripletQuery<uint>.Any));
                var r = Named(triplets, "r", "knows", "alice", "bob");
                Assert.Equal(r, triplets.GetTriplet(r.Index));
                Assert.Equal(People_, TripletsLino.Export(triplets));
            }

            // Without names a triplet is its address, and the import keeps it.
            using (var numberedStore = new Store())
            using (var copyStore = new Store())
            {
                var numbered = numberedStore.Triplets;
                var p = CreatePeople(numbered);
                numbered.CreateTriplet(p.Knows, p.Alice, p.Bob);
                var gone = numbered.CreateTriplet(p.Knows, p.Alice, p.Carol);
                numbered.CreateTriplet(p.Knows, p.Alice, p.Bob);
                numbered.CreateTriplet(p.Knows, 0, p.Carol);
                numbered.DeleteTriplet(gone);
                var text = TripletsLino.Export(numbered);
                var copy = copyStore.Triplets;
                var imported = TripletsLino.Import(copy, text);
                Assert.Equal(numbered.CountTriplets(TripletQuery<uint>.Any), imported.Count);
                Assert.Equal(numbered.EachTriplet(TripletQuery<uint>.Any), copy.EachTriplet(TripletQuery<uint>.Any));
                Assert.Equal(text, TripletsLino.Export(copy));
                AssertNoOrphans(copy, false);
            }

            // Lines may come in any order, and a line without an index is new.
            using (var forwardStore = new Store())
            {
                var forward = forwardStore.Triplets;
                var imported = TripletsLino.Import(forward, "(r: knows alice bob)\n(knows: Type 0 0)\n(alice: Type 0 0)\n(bob: Type 0 0)\n(Type: Type 0 0)\n(knows bob alice)\n");
                Assert.Equal(6, imported.Count);
                Assert.Equal(Named(forward, "r", "knows", "alice", "bob"), forward.GetTriplet(imported[0]));
                var last = forward.GetTriplet(imported[5])!.Value;
                Assert.Equal((Id(forward, "knows"), Id(forward, "bob"), Id(forward, "alice")), (last.Type, last.From, last.To));
                AssertNoOrphans(forward, false);
            }
        }

        [Fact]
        public void Lino_Import_Refuses_What_Is_Not_A_Triplet()
        {
            using var store = NamedPeople();
            var triplets = store.Triplets;
            var markers = triplets.Markers;
            var before = TripletsLino.Export(triplets);
            foreach (var text in new[]
            {
                "(x: nobody 0 0)",
                "(x: Type 0)",
                "(x: $t 0 0)",
                "(x: Type * 0)",
                "(x: 0 0 0)",
                $"({markers.Pair}: Type 0 0)",
                $"(x: Type {markers.Nothing} 0)",
            })
            {
                Assert.ThrowsAny<Exception>(() => TripletsLino.Import(triplets, text));
            }
            Assert.Equal(before, TripletsLino.Export(triplets));
        }

        [Theory]
        [InlineData("'0'", "0")]
        [InlineData("\"1\"", "1")]
        [InlineData("`9999`", "9999")]
        [InlineData("'*'", "*")]
        [InlineData("'$item'", "$item")]
        [InlineData("~1{30}", "0")]
        [InlineData("'\"0\"'", "\"0\"")]
        [InlineData("'x\\y'", "x\\y")]
        [InlineData("'a\nb'", "a\nb")]
        [InlineData("'#tag'", "#tag")]
        [InlineData("'~value'", "~value")]
        [InlineData("'  '", "  ")]
        [InlineData("\"a'b`c\"", "a'b`c")]
        [InlineData("`a'b\"c`", "a'b\"c")]
        public void Quoted_Triplet_References_Remain_Names_In_Imports_And_Queries(string reference, string name)
        {
            using var store = new Store();
            var triplets = store.Triplets;
            TripletsLino.Import(triplets, $"(Type: Type 0 0)\n({reference}: Type 0 0)\n(r: {reference} {reference} {reference})\n");
            var namedId = Id(triplets, name);
            var r = Id(triplets, "r");
            var before = T(r, namedId, namedId, namedId);
            Assert.Equal(before, triplets.GetTriplet(r));

            var text = TripletsLino.Export(triplets);
            using var copyStore = new Store();
            TripletsLino.Import(copyStore.Triplets, text);
            Assert.Equal(text, TripletsLino.Export(copyStore.Triplets));
            Assert.NotEqual(0u, copyStore.Triplets.GetByName(name));

            using var createdStore = new Store();
            TripletsQueryProcessor.ProcessQuery(createdStore.Triplets,
                $"() ((Type: Type 0 0) ({reference}: Type 0 0) (r: {reference} {reference} {reference}))");
            Assert.Equal(text, TripletsLino.Export(createdStore.Triplets));
            AssertNoOrphans(createdStore.Triplets, false);

            Assert.Equal(new[] { r }, TripletsQueryProcessor.MatchQuery(triplets,
                $"(($r: {reference} {reference} {reference}))", "$r"));
            var changes = TripletsQueryProcessor.ProcessQuery(triplets,
                $"((r: {reference} {reference} {reference})) ((r: {reference} 0 {reference}))");
            Assert.Equal(new[] { new TripletChange<uint>(before, T(r, namedId, 0, namedId)) }, changes);
            TripletsQueryProcessor.ProcessQuery(triplets, $"() (({reference}: Type 0 0))");
            Assert.Equal(namedId, Id(triplets, name));
            AssertNoOrphans(triplets, false);
        }

        private static IReadOnlyList<TripletChange<uint>> Query(ITriplets<uint> triplets, string text)
        {
            var changes = TripletsQueryProcessor.ProcessQuery(triplets, text);
            AssertNoOrphans(triplets, false);
            return changes;
        }

        [Fact]
        public void Substitution_Updates_The_To_Part_In_Place()
        {
            using var store = NamedPeople();
            var triplets = store.Triplets;
            var before = Named(triplets, "r", "knows", "alice", "bob");
            var after = Named(triplets, "r", "knows", "alice", "carol");
            // The query of the issue: ((i: T f t)) ((i: T f t2)), by address.
            var text = $"((({before.Index}: {before.Type} {before.From} {before.To})) (({after.Index}: {after.Type} {after.From} {after.To})))";
            var changes = Query(triplets, text);
            Assert.Equal(new[] { new TripletChange<uint>(before, after) }, changes);
            Assert.Equal(after, triplets.GetTriplet(before.Index));
            // And by name, with a variable.
            changes = Query(triplets, "((($r: knows alice carol)) (($r: knows alice bob)))");
            Assert.Equal(before, changes[0].After);
        }

        [Fact]
        public void Substitution_Reads_Creates_And_Deletes()
        {
            using var store = NamedPeople();
            var triplets = store.Triplets;
            var r = Named(triplets, "r", "knows", "alice", "bob");
            // A substitution that repeats the restriction only reads.
            var read = Query(triplets, "((($i: knows $f $t)) (($i: knows $f $t)))");
            Assert.Equal(new[] { new TripletChange<uint>(r, r) }, read);
            var peopleRead = Query(triplets, "((($p: Person 0 0)) (($p: Person 0 0)))");
            Assert.Equal(3, peopleRead.Count);

            // A create for every match, with the variable of the match.
            var created = Query(triplets, "((($p: Person 0 0)) (($p: Person 0 0) (knows $p bob)))");
            var fresh = created.Where(change => change.Before is null).Select(change => change.After!.Value).ToList();
            Assert.Equal(3, fresh.Count);
            var bob = Id(triplets, "bob");
            Assert.Equal(4, triplets.CountTriplets(TripletQuery<uint>.ByTo(bob)));

            // No restriction: a create; a new name is a new triplet.
            var changes = Query(triplets, "(() ((likes: Type 0 0) (likes carol alice)))");
            Assert.Equal(2, changes.Count);
            Assert.All(changes, change => Assert.Null(change.Before));
            var likes = Id(triplets, "likes");
            Assert.Equal(1, triplets.CountTriplets(TripletQuery<uint>.ByType(likes)));

            // No substitution: a delete, with whatever used the deleted triplet.
            var deleted = Query(triplets, "((($i: knows $p bob)) ())");
            Assert.Equal(4, deleted.Count);
            Assert.All(deleted, change => Assert.Null(change.After));
            Assert.Equal(0, triplets.CountTriplets(TripletQuery<uint>.ByTo(bob)));
            Assert.False(triplets.IsTriplet(r.Index));
        }

        [Fact]
        public void Substitution_Changes_The_Type_Part()
        {
            using var store = NamedPeople();
            var triplets = store.Triplets;
            Query(triplets, "(() ((likes: Type 0 0)))");
            var changes = Query(triplets, "((($i: knows $f $t)) (($i: likes $f $t)))");
            Assert.Equal(Named(triplets, "r", "likes", "alice", "bob"), changes[0].After);
            // `*` keeps a part, and the parts may swap.
            changes = Query(triplets, "((($i: likes $f $t)) (($i: * $t $f)))");
            Assert.Equal(Named(triplets, "r", "likes", "bob", "alice"), changes[0].After);
            // A conjunction: who is liked by someone of type Person.
            changes = Query(triplets, "((($i: likes $f $t) ($f: Person 0 0)) (($i: likes $f $t) ($f: Person 0 0)))");
            Assert.Equal(2, changes.Count);
        }

        [Fact]
        public void Substitution_Refuses_Unknown_References()
        {
            using var store = NamedPeople();
            var triplets = store.Triplets;
            var before = TripletsLino.Export(triplets);
            foreach (var text in new[]
            {
                "(() ((nobody alice bob)))",
                "(() ((knows $x bob)))",
                "(() ((knows * bob)))",
                "(() ((*: knows alice bob)))",
                "(() ((knows alice)))",
                "(() ((0 alice bob)))",
                "((($i: knows $f $t)) (($i: knows $f $nobody)))",
            })
            {
                Assert.ThrowsAny<Exception>(() => TripletsQueryProcessor.ProcessQuery(triplets, text));
            }
            Assert.Equal(before, TripletsLino.Export(triplets));
            // A restriction by an unknown name matches nothing.
            Assert.Empty(Query(triplets, "((($i: nobody $f $t)) ())"));
        }

        /// <summary>
        /// The triplets are plain doublets, so a file-backed store keeps them across
        /// a reopen and the named markers are found again.
        /// </summary>
        [Fact]
        public void Keeps_Triplets_In_A_Named_File_Store()
        {
            var file = Path.GetTempFileName();
            var namesFile = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(file);
            try
            {
                string text;
                using (var named = new NamedTypesDecorator<uint>(file))
                {
                    var triplets = new TripletsOverDoublets<uint>(named);
                    TripletsLino.Import(triplets, People_);
                    text = TripletsLino.Export(triplets);
                }
                using (var named = new NamedTypesDecorator<uint>(file))
                {
                    var triplets = new TripletsOverDoublets<uint>(named);
                    Assert.Equal(text, TripletsLino.Export(triplets));
                    Assert.Equal(People_, text);
                }
            }
            finally
            {
                File.Delete(file);
                File.Delete(namesFile);
            }
        }
    }
}
