using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class UnicodeStringStorageTests
    {
        [Fact]
        public void CreateAndRetrieveEmptyStringTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                // Act
                var emptyStringLink = storage.CreateString("");
                var retrievedString = storage.GetString(emptyStringLink);

                // Assert
                Assert.Equal("", retrievedString);
            });
        }

        [Fact]
        public void CreateAndRetrieveSimpleStringTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                // Act
                var helloLink = storage.CreateString("Hello");
                var retrievedHello = storage.GetString(helloLink);

                // Assert
                Assert.Equal("Hello", retrievedHello);
            });
        }

        [Fact]
        public void CreateAndRetrieveMultipleStringsTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                // Act
                var linkOne = storage.CreateString("First");
                var linkTwo = storage.CreateString("Second");
                var retrievedOne = storage.GetString(linkOne);
                var retrievedTwo = storage.GetString(linkTwo);

                // Assert
                Assert.Equal("First", retrievedOne);
                Assert.Equal("Second", retrievedTwo);
            });
        }

        [Fact]
        public void CreateAndRetrieveUnicodeStringTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                // A string with some unicode characters
                var unicodeContent = "Hello, 世界! Привет, мир!";

                // Act
                var unicodeLink = storage.CreateString(unicodeContent);
                var retrieved = storage.GetString(unicodeLink);

                // Assert
                Assert.Equal(unicodeContent, retrieved);
            });
        }

        [Fact]
        public void RetrieveTypeByNameTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                // Act
                var retrievedType = storage.NamedLinks.GetByName("Type");

                // Assert
                Assert.Equal(1u, retrievedType);
            });
        }

        [Theory]
        [InlineData("Type")]
        [InlineData("UnicodeSymbol")]
        [InlineData("UnicodeSequence")]
        [InlineData("String")]
        [InlineData("EmptyString")]
        [InlineData("Name")]
        public void CreateAndRetrieveMultipleStringTypesTest(string typeName)
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                // Act
                var retrievedType = storage.NamedLinks.GetByName(typeName);

                // Assert
                Assert.Equal(typeName, storage.NamedLinks.GetName(retrievedType));
            });
        }

        [Fact]
        public void CreateAndRetriveUserDefinedTypeTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                // Act
                var userType = storage.GetOrCreateType("UserType");
                var retrievedType = storage.NamedLinks.GetByName("UserType");

                // Assert
                Assert.Equal(userType, retrievedType);
            });
        }

        [Fact]
        public void NameExternalReferenceTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);

                Hybrid<uint> hybrid = new Hybrid<uint>(1, isExternal: true);

                // Act
                storage.NamedLinks.SetName(hybrid, "MyExternalReference");
                var retrievedName = storage.NamedLinks.GetName(hybrid);
                var retrievedIndex = storage.NamedLinks.GetByName("MyExternalReference");

                // Assert
                Assert.Equal(4294967295ul, (ulong)hybrid);
                Assert.Equal(4294967295, retrievedIndex);
                Assert.Equal(1ul, (ulong)hybrid.AbsoluteValue);
                Assert.True(hybrid.IsExternal);
                Assert.Equal("MyExternalReference", retrievedName);
            });
        }

        [Fact]
        public void NameIsRemovedWhenLinkIsDeletedTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);
                var link = links.Create(new uint[] { 0, 0 });
                storage.NamedLinks.SetName(link, "TestName");
                Assert.Equal("TestName", storage.NamedLinks.GetName(link));
                Assert.Equal(link, storage.NamedLinks.GetByName("TestName"));

                // Act: delete the link and its name
                links.Delete(link);
                storage.NamedLinks.RemoveName(link);

                // Assert: name is removed
                Assert.Equal(links.Constants.Null, storage.NamedLinks.GetByName("TestName"));
                Assert.Null(storage.NamedLinks.GetName(link));
            });
        }

        [Fact]
        public void DeletingNonNamedLinkDoesNotAffectOtherNamesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);
                var namedLink = links.Create(new uint[] { 0, 0 });
                storage.NamedLinks.SetName(namedLink, "Named");
                var unnamedLink = links.Create(new uint[] { 0, 0 });
                Assert.Equal("Named", storage.NamedLinks.GetName(namedLink));

                // Act: delete the unnamed link
                links.Delete(unnamedLink);

                // Assert: named link and its name remain
                Assert.Equal(namedLink, storage.NamedLinks.GetByName("Named"));
                Assert.Equal("Named", storage.NamedLinks.GetName(namedLink));
            });
        }

        [Fact]
        public void NameIsRemovedWhenExternalReferenceIsDeletedTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                var storage = new UnicodeStringStorage<uint>(links);
                var externalRef = 123u;
                storage.NamedLinks.SetNameForExternalReference(externalRef, "ExternalName");
                Assert.Equal("ExternalName", storage.NamedLinks.GetNameByExternalReference(externalRef));
                Assert.Equal(externalRef, storage.NamedLinks.GetExternalReferenceByName("ExternalName"));

                // Act: remove the name for the external reference (do not call links.Delete for external references)
                storage.NamedLinks.RemoveNameByExternalReference(externalRef);

                // Assert: name is removed
                Assert.Equal(links.Constants.Null, storage.NamedLinks.GetExternalReferenceByName("ExternalName"));
                Assert.Null(storage.NamedLinks.GetNameByExternalReference(externalRef));
            });
        }

        // Helper method to create a test environment with a temporary file
        private static void RunTestWithLinks(Action<ILinks<uint>> testAction)
        {
            var tempDbFile = Path.GetTempFileName();
            try
            {
                var constants = new LinksConstants<uint>(enableExternalReferencesSupport: true);
                var memory = new FileMappedResizableDirectMemory(tempDbFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var links = new UnitedMemoryLinks<uint>(memory, UnitedMemoryLinks<uint>.DefaultLinksSizeStep, constants, Platform.Data.Doublets.Memory.IndexTreeType.Default);
                var decoratedLinks = links.DecorateWithAutomaticUniquenessAndUsagesResolution();
                testAction(decoratedLinks);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }
    }
}