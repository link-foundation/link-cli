// Edges of the query processors: query shapes that are refused, references with an
// index suffix, and substitutions that change nothing.
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public sealed class QueryProcessorEdgesTests
    {
        private sealed class Store : IDisposable
        {
            private readonly string _dataFile = Path.GetTempFileName();
            private readonly string _namesFile = Path.GetTempFileName();
            private readonly FileMappedResizableDirectMemory _memory;
            private readonly UnitedMemoryLinks<uint> _united;
            private readonly Lazy<TripletsOverDoublets<uint>> _triplets;

            public Store()
            {
                _memory = new FileMappedResizableDirectMemory(_dataFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                _united = new UnitedMemoryLinks<uint>(_memory);
                Named = new NamedTypesDecorator<uint>(_united.DecorateWithAutomaticUniquenessAndUsagesRepointing(), _namesFile);
                _triplets = new Lazy<TripletsOverDoublets<uint>>(() => new TripletsOverDoublets<uint>(Named));
            }

            public NamedTypesDecorator<uint> Named { get; }

            // Opening triplets adds their marker links, which the doublet tests do not want.
            public TripletsOverDoublets<uint> Triplets => _triplets.Value;

            public void Dispose()
            {
                Named.Dispose();
                _united.Dispose();
                _memory.Dispose();
                File.Delete(_dataFile);
                File.Delete(_namesFile);
            }
        }

        private static List<DoubletLink> Rows(ILinks<uint> links)
        {
            var rows = new List<DoubletLink>();
            links.Each(new DoubletLink(links.Constants.Any, links.Constants.Any, links.Constants.Any), row =>
            {
                rows.Add(new DoubletLink(row));
                return links.Constants.Continue;
            });
            return rows;
        }

        // TripletsQueryProcessor

        [Theory]
        [InlineData("(a b c)\n(d e f)\n(g h i)")]
        [InlineData("(x: (Type 0 0) (Type 0 0))")]
        [InlineData("((Type 0 0))")]
        [InlineData("(Type 0 0) (Type 0 0) (Type 0 0)")]
        public void Triplets_Query_Of_Another_Shape_Is_Refused(string query)
        {
            using var store = new Store();
            var triplets = store.Triplets;
            var before = Rows(store.Named);
            var refused = Assert.Throws<ArgumentException>(() => TripletsQueryProcessor.ProcessQuery(triplets, query));
            Assert.Contains("((restriction) (substitution))", refused.Message);
            Assert.Equal(before, Rows(store.Named));
        }

        [Fact]
        public void Triplets_Empty_Query_Changes_Nothing()
        {
            using var store = new Store();
            Assert.Empty(TripletsQueryProcessor.ProcessQuery(store.Triplets, ""));
            Assert.Empty(TripletsQueryProcessor.ProcessQuery(store.Triplets, null!));
        }

        [Fact]
        public void Triplets_Restriction_And_Substitution_May_Be_Two_Lines()
        {
            using var store = new Store();
            Assert.Single(TripletsQueryProcessor.ProcessQuery(store.Triplets, "()\n((Type: Type 0 0))"));
        }

        [Fact]
        public void Triplets_Restriction_Is_One_Link_Of_Patterns()
        {
            using var store = new Store();
            var refused = Assert.Throws<ArgumentException>(() =>
                TripletsQueryProcessor.MatchQuery(store.Triplets, "(a b c)\n(d e f)", "$x"));
            Assert.Contains("one link of patterns", refused.Message);
        }

        [Fact]
        public void Triplets_Match_Without_The_Variable_Finds_Nothing()
        {
            using var store = new Store();
            TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((Type: Type 0 0))");
            Assert.Empty(TripletsQueryProcessor.MatchQuery(store.Triplets, "(($x: Type 0 0))", "$other"));
        }

        [Fact]
        public void Triplets_Part_Of_A_Pattern_Must_Be_A_Reference()
        {
            using var store = new Store();
            var refused = Assert.Throws<ArgumentException>(() =>
                TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((Type (a b) 0))"));
            Assert.Contains("is a reference", refused.Message);
        }

        [Fact]
        public void Triplets_Cannot_Be_At_Address_Zero()
        {
            using var store = new Store();
            var refused = Assert.Throws<ArgumentException>(() =>
                TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((Type: Type 0 0) (0: Type 0 0))"));
            Assert.Contains("address 0", refused.Message);
        }

        [Fact]
        public void Triplets_Name_Of_A_Doublet_Cannot_Be_Written_As_A_Triplet()
        {
            using var store = new Store();
            TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((Type: Type 0 0))");
            var doublet = store.Named.GetOrCreate(store.Named.GetByName("Type"), 0u);
            store.Named.SetName(doublet, "plain");
            var refused = Assert.Throws<InvalidOperationException>(() =>
                TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((plain: Type 0 0))"));
            Assert.Contains("is not a triplet", refused.Message);
        }

        [Fact]
        public void Triplets_New_Variable_Defined_Twice_Is_One_Triplet()
        {
            using var store = new Store();
            TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((Type: Type 0 0))");
            var changes = TripletsQueryProcessor.ProcessQuery(store.Triplets, "() (($v: Type 0 0) ($v: Type $v 0))");
            var change = Assert.Single(changes);
            Assert.Null(change.Before);
            var after = Assert.IsType<Triplet<uint>>(change.After);
            Assert.Equal(after.Index, after.From);
        }

        [Fact]
        public void Triplets_New_Name_Defined_Twice_Is_One_Triplet()
        {
            using var store = new Store();
            var changes = TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((Type: Type 0 0) (r: Type 0 0) (r: Type r 0))");
            Assert.Equal(2, changes.Count);
            var r = store.Triplets.GetByName("r");
            Assert.Equal(r, changes[1].After!.Value.From);
        }

        [Fact]
        public void Triplets_Written_By_Two_Patterns_Is_Reported_Once_As_It_Ends_Up()
        {
            using var store = new Store();
            TripletsQueryProcessor.ProcessQuery(store.Triplets, "() ((Type: Type 0 0) (r: Type Type Type))");
            var r = store.Triplets.GetByName("r");
            var type = store.Triplets.GetByName("Type");
            var changes = TripletsQueryProcessor.ProcessQuery(store.Triplets, "((r: Type Type Type)) ((r: Type 0 Type) (r: Type 0 0))");
            var change = Assert.Single(changes);
            Assert.Equal(new Triplet<uint>(r, type, type, type), change.Before);
            Assert.Equal(new Triplet<uint>(r, type, 0, 0), change.After);
        }

        // AdvancedMixedQueryProcessor

        private static List<(DoubletLink Before, DoubletLink After)> Advanced(NamedTypesDecorator<uint> links, string query, bool skipReferenceValidation = false)
        {
            var changes = new List<(DoubletLink, DoubletLink)>();
            Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.ProcessQuery(links, new Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.Options
            {
                Query = query,
                SkipReferenceValidation = skipReferenceValidation,
                ChangesHandler = (before, after) =>
                {
                    changes.Add((new DoubletLink(before), new DoubletLink(after)));
                    return links.Constants.Continue;
                },
            });
            return changes;
        }

        [Fact]
        public void Advanced_Creating_What_Exists_Reports_It_Unchanged()
        {
            using var store = new Store();
            Advanced(store.Named, "(() ((1 1)))");
            var row = new DoubletLink(1, 1, 1);
            Assert.Contains((row, row), Advanced(store.Named, "(() ((1 1)))"));
            Assert.Contains((row, row), Advanced(store.Named, "(() ((1: 1 1)))"));
            Assert.Equal(new[] { row }, Rows(store.Named));
        }

        [Fact]
        public void Advanced_Restriction_And_Substitution_With_An_Id_Are_Extra_Patterns()
        {
            using var store = new Store();
            Advanced(store.Named, "(() ((1 1)))");
            Advanced(store.Named, "((1: ((1: 1 1))) (1: ((1: 1 1))))");
            Assert.Equal(new[] { new DoubletLink(1, 1, 1) }, Rows(store.Named));
        }

        [Fact]
        public void Advanced_Index_Suffix_In_A_Reference_Means_The_Address()
        {
            using var store = new Store();
            Advanced(store.Named, "(() ((1 1)))");
            Assert.Throws<InvalidOperationException>(() => Advanced(store.Named, "(((1: '1:' 1)) ((1: 1 1)))"));
            Advanced(store.Named, "(() ((2 2)))");
            Advanced(store.Named, "(((1: '1:' 1)) ((1: 1 2)))", skipReferenceValidation: true);
            Assert.Contains(new DoubletLink(1, 1, 2), Rows(store.Named));
        }

        [Fact]
        public void Advanced_Nested_Link_Of_Three_Parts_Stands_For_Any()
        {
            using var store = new Store();
            Advanced(store.Named, "(() ((1 1)))");
            Advanced(store.Named, "(() (((1 1 1) 1)))");
            Assert.Contains(new DoubletLink(1, 1, 1), Rows(store.Named));
        }

        [Fact]
        public void Advanced_Moving_A_Link_To_Another_Address_Keeps_Its_Parts()
        {
            using var store = new Store();
            Advanced(store.Named, "(() ((1 1)))");
            Advanced(store.Named, "(((1: 1 1)) ((5: 1 1)))");
            var rows = Rows(store.Named);
            Assert.Contains(rows, row => row.Index == 5);
        }

        // MixedQueryProcessor

        private static void Mixed(ILinks<uint> links, string query) =>
            Foundation.Data.Doublets.Cli.MixedQueryProcessor.ProcessQuery(links, query);

        [Fact]
        public void Mixed_Restriction_And_Substitution_With_An_Id_Are_Kept_By_Id()
        {
            using var store = new Store();
            Mixed(store.Named, "(() ((1 1)))");
            Mixed(store.Named, "(() ((2 2)))");
            Mixed(store.Named, "((1: 1 2) (1: 2 1))");
            Assert.Contains(new DoubletLink(1, 2, 1), Rows(store.Named));
        }

        [Fact]
        public void Mixed_Index_Suffix_In_A_Reference_Means_The_Address()
        {
            using var store = new Store();
            Mixed(store.Named, "(() ((1 1)))");
            Mixed(store.Named, "(() ((2: '1:' 2)))");
            Assert.Contains(new DoubletLink(2, 1, 2), Rows(store.Named));
        }

        [Fact]
        public void Mixed_Variables_Without_An_Index_Rewrite_The_Substitution()
        {
            using var store = new Store();
            Mixed(store.Named, "(() ((1 1)))");
            Mixed(store.Named, "((($s $t)) (($s $t)))");
            Assert.Equal(new[] { new DoubletLink(1, 1, 1) }, Rows(store.Named));
        }
    }
}
