// A query reports every link it creates, names or redefines, and refers only
// to links that exist once it is done.
using DoubletLink = Platform.Data.Doublets.Link<uint>;

using static Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;
namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public partial class AdvancedMixedQueryProcessor
    {
        private static readonly DoubletLink NoLink = new(0, 0, 0);

        private static List<(DoubletLink Before, DoubletLink After)> ReportedChanges(NamedTypesDecorator<uint> links, string query)
        {
            var changes = new List<(DoubletLink Before, DoubletLink After)>();
            ProcessQuery(links, new Options
            {
                Query = query,
                ChangesHandler = (before, after) =>
                {
                    changes.Add((new DoubletLink(before), new DoubletLink(after)));
                    return links.Constants.Continue;
                }
            });
            return Sorted(Foundation.Data.Doublets.Cli.ChangesSimplifier.SimplifyChanges(changes));
        }

        private static List<(DoubletLink Before, DoubletLink After)> Sorted(IEnumerable<(DoubletLink Before, DoubletLink After)> changes) =>
            changes.OrderBy(change => change.Before.Index).ThenBy(change => change.After.Index).ToList();

        private static (DoubletLink Before, DoubletLink After) Created(uint index, uint source, uint target) =>
            (NoLink, new DoubletLink(index, source, target));

        private static uint Named(NamedTypesDecorator<uint> links, string name)
        {
            var index = links.GetByName(name);
            Assert.NotEqual(links.Constants.Null, index);
            return index;
        }

        [Fact]
        public void NumericPointLinkIsReportedCreated()
        {
            RunTestWithLinks(links =>
            {
                Assert.Equal([Created(2, 2, 2)], ReportedChanges(links, "() ((2: 2 2))"));
            });
        }

        [Fact]
        public void NamedPointLinkIsReportedCreated()
        {
            RunTestWithLinks(links =>
            {
                var changes = ReportedChanges(links, "() ((a: a a))");
                var a = Named(links, "a");
                Assert.Equal([Created(a, a, a)], changes);
            });
        }

        [Fact]
        public void NamedLeafCreatedOnTheWayIsReportedCreated()
        {
            RunTestWithLinks(links =>
            {
                var changes = ReportedChanges(links, "() ((c: c d))");
                var (c, d) = (Named(links, "c"), Named(links, "d"));
                Assert.Equal(Sorted([Created(c, c, d), Created(d, d, d)]), changes);
            });
        }

        [Fact]
        public void EveryNamedLeafOfACompositeIsReportedCreated()
        {
            RunTestWithLinks(links =>
            {
                var changes = ReportedChanges(links, "() ((child: father mother))");
                var (child, father, mother) = (Named(links, "child"), Named(links, "father"), Named(links, "mother"));
                Assert.Equal(Sorted([Created(child, father, mother), Created(father, father, father), Created(mother, mother, mother)]), changes);
                Assert.Equal(3, GetAllLinks(links).Count);
            });
        }

        [Fact]
        public void CreatingAnExistingNameRedefinesThatLink()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "() ((child: father mother))");
                var (child, father, mother) = (Named(links, "child"), Named(links, "father"), Named(links, "mother"));

                Assert.Equal(
                    [(new DoubletLink(child, father, mother), new DoubletLink(child, mother, father))],
                    ReportedChanges(links, "() ((child: mother father))"));
                Assert.Equal(3, GetAllLinks(links).Count);
            });
        }

        [Fact]
        public void NewNameForAnExistingDoubletNamesItInsteadOfCopyingIt()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "() ((a: a a))");
                var a = Named(links, "a");
                var point = new DoubletLink(a, a, a);

                Assert.Equal([(point, point)], ReportedChanges(links, "() ((b: a a))"));
                Assert.Equal(a, Named(links, "b"));
                Assert.Equal([point], GetAllLinks(links));
            });
        }

        [Fact]
        public void NumericReferenceIsReportedCreatedAsThePointLink()
        {
            RunTestWithLinks(links =>
            {
                Assert.Equal([Created(2, 2, 2), Created(3, 3, 2)], ReportedChanges(links, "() ((3: 3 2))"));
            });
        }

        /// <summary>
        /// Doublets are unique, so the point link (2: 2 2) and the defined (3: 2 2) cannot both exist:
        /// the reference is left empty.
        /// </summary>
        [Fact]
        public void ReferenceToThePairALinkDefinesIsReportedCreatedEmpty()
        {
            RunTestWithLinks(links =>
            {
                Assert.Equal([Created(2, 0, 0), Created(3, 2, 2)], ReportedChanges(links, "() ((3: 2 2))"));
                Assert.Equal([new DoubletLink(2, 0, 0), new DoubletLink(3, 2, 2)], GetAllLinks(links));
            });
        }

        /// <summary>Links 1 to 4, with 2 and then 3 deleted: the store hands out 3, the address freed last, before 2.</summary>
        private static void FreeAddresses2And3(NamedTypesDecorator<uint> links)
        {
            ProcessQueryStrict(links, "() ((1 1) (2 2) (3 3) (4 4))");
            ProcessQueryStrict(links, "((2: 2 2)) ()");
            ProcessQueryStrict(links, "((3: 3 3)) ()");
        }

        [Fact]
        public void ReferenceToAFreedAddressTheNewLinkDoesNotGetFails()
        {
            RunTestWithLinks(links =>
            {
                FreeAddresses2And3(links);

                var exception = Assert.Throws<InvalidOperationException>(() => ProcessQueryStrict(links, "() ((2 2))"));

                Assert.Contains("'2'", exception.Message);
                Assert.Equal(2, GetAllLinks(links).Count);
            });
        }

        [Fact]
        public void AutoCreateAReferenceToAFreedAddressTheNewLinkDoesNotGet()
        {
            RunTestWithLinks(links =>
            {
                FreeAddresses2And3(links);

                ProcessQuery(links, "() ((2 2))");

                var allLinks = GetAllLinks(links);
                AssertLinkExists(allLinks, 2, 2, 2);
                Assert.Equal(3, allLinks.Count);
            });
        }

        [Fact]
        public void AutoCreateAReferenceTheNewLinkWouldHaveGotBefore()
        {
            RunTestWithLinks(links =>
            {
                ProcessQueryStrict(links, "() ((1 1) (2 2) (3 3) (4 4))");

                // Creating 7 frees 5 and 6 on the way, so (5 7) gets 6, not 5.
                ProcessQuery(links, "() ((5 7))");

                var allLinks = GetAllLinks(links);
                AssertLinkExists(allLinks, 5, 5, 5);
                AssertLinkExists(allLinks, 6, 5, 7);
                AssertLinkExists(allLinks, 7, 7, 7);
                Assert.Equal(7, allLinks.Count);
            });
        }
    }
}
