using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public class PersistentTransformationDecoratorTests
    {
        [Fact]
        public void AlwaysTriggerIsStoredInLinksAndAppliedAfterWrite()
        {
            RunWithPersistentLinks((links, triggerLinks) =>
            {
                links.StoreTrigger(PersistentTransformationKind.Always, "(((1: 1 1)) ((1: 1 2)))");

                var allTriggerLinks = AllLinks(triggerLinks);
                var alwaysId = triggerLinks.GetByName("Always");
                Assert.NotEqual(triggerLinks.Constants.Null, alwaysId);
                Assert.Contains(allTriggerLinks, link => link.Source == alwaysId && link.Target != alwaysId);

                Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.ProcessQuery(links, new Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.Options
                {
                    Query = "(() ((1: 1 1)))",
                    AutoCreateMissingReferences = true
                });

                Assert.Contains(AllLinks(links), link => link.Index == 1 && link.Source == 1 && link.Target == 2);
            });
        }

        [Fact]
        public void OnceTriggerDeletesItselfAfterFirstMatch()
        {
            RunWithPersistentLinks((links, triggerLinks) =>
            {
                links.StoreTrigger(PersistentTransformationKind.Once, "(((1: 1 1)) ((1: 1 2)))");

                Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.ProcessQuery(links, new Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.Options
                {
                    Query = "(() ((1: 1 1)))",
                    AutoCreateMissingReferences = true
                });

                Assert.DoesNotContain(links.GetTriggers(), trigger => trigger.Kind == PersistentTransformationKind.Once);

                Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.ProcessQuery(links, new Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.Options
                {
                    Query = "(((1: 1 2)) ((1: 1 1)))",
                    AutoCreateMissingReferences = true
                });

                Assert.Contains(AllLinks(links), link => link.Index == 1 && link.Source == 1 && link.Target == 1);
            });
        }

        [Fact]
        public void NeverRemovesMatchingStoredTrigger()
        {
            RunWithPersistentLinks((links, triggerLinks) =>
            {
                links.StoreTrigger(PersistentTransformationKind.Always, "(((1: 1 1)) ((1: 1 2)))");

                var removed = links.RemoveTriggers("(((1: 1 1)) ((1: 1 2)))");

                Assert.Equal(1, removed);
                Assert.Empty(links.GetTriggers());
            });
        }

        private static void RunWithPersistentLinks(Action<PersistentTransformationDecorator, NamedTypesDecorator<uint>> action)
        {
            var dataFile = Path.GetTempFileName();
            var triggerFile = Path.GetTempFileName();
            var dataNamesFile = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dataFile);
            var triggerNamesFile = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(triggerFile);
            try
            {
                // Both decorators are disposed at the end of the try block, before the finally deletes the
                // backing files: Windows refuses to delete a file that is still memory-mapped.
                using var dataLinks = new NamedTypesDecorator<uint>(dataFile);
                using var triggerLinks = new NamedTypesDecorator<uint>(triggerFile);
                var links = new PersistentTransformationDecorator(dataLinks, triggerLinks)
                {
                    AutoCreateMissingReferences = true
                };

                action(links, triggerLinks);
            }
            finally
            {
                DeleteIfExists(dataFile);
                DeleteIfExists(triggerFile);
                DeleteIfExists(dataNamesFile);
                DeleteIfExists(triggerNamesFile);
            }
        }

        private static List<DoubletLink> AllLinks(INamedTypesLinks<uint> links)
        {
            var any = links.Constants.Any;
            return links.All(new DoubletLink(any, any, any)).Select(link => new DoubletLink(link)).ToList();
        }

        private static void DeleteIfExists(string path)
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
    }
}
