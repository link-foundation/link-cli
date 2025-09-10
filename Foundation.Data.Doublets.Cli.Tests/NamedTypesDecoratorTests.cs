using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using Xunit;
using Platform.Memory;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Foundation.Data.Doublets.Cli;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class NamedTypesDecoratorTests : IDisposable
    {
        private readonly string _tempDbPath;
        private readonly string _tempNamesDbPath;

        public NamedTypesDecoratorTests()
        {
            _tempDbPath = Path.GetTempFileName();
            _tempNamesDbPath = Path.GetTempFileName();
        }

        public void Dispose()
        {
            if (File.Exists(_tempDbPath)) File.Delete(_tempDbPath);
            if (File.Exists(_tempNamesDbPath)) File.Delete(_tempNamesDbPath);
        }

        private static void RunTestWithLinks(Action<ILinks<uint>> testAction)
        {
            var tempFile = Path.GetTempFileName();
            try
            {
                using var memory = new FileMappedResizableDirectMemory(tempFile, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var unitedMemoryLinks = new UnitedMemoryLinks<uint>(memory);
                var linksDecoratedWithAutomaticUniquenessResolution = unitedMemoryLinks.DecorateWithAutomaticUniquenessAndUsagesResolution();
                testAction(linksDecoratedWithAutomaticUniquenessResolution);
            }
            finally
            {
                if (File.Exists(tempFile)) File.Delete(tempFile);
            }
        }

        [Fact]
        public void NamedTypesDecorator_ImplementsILinks()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                Assert.True(decorator is ILinks<uint>);
            });
        }

        [Fact]
        public void NamedTypesDecorator_ImplementsINamedTypes()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                Assert.True(decorator is INamedTypes<uint>);
            });
        }

        [Fact]
        public void NamedTypesDecorator_CanSetAndGetNames()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                var link1 = decorator.GetOrCreate(10u, 20u);
                var link2 = decorator.GetOrCreate(30u, 40u);
                
                var nameLink1 = decorator.SetName(link1, "TestLink1");
                var nameLink2 = decorator.SetName(link2, "TestLink2");
                
                Assert.NotEqual(links.Constants.Null, nameLink1);
                Assert.NotEqual(links.Constants.Null, nameLink2);
                
                var retrievedName1 = decorator.GetName(link1);
                var retrievedName2 = decorator.GetName(link2);
                
                Assert.Equal("TestLink1", retrievedName1);
                Assert.Equal("TestLink2", retrievedName2);
            });
        }

        [Fact]
        public void NamedTypesDecorator_CanGetLinkByName()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                var link = decorator.GetOrCreate(50u, 60u);
                decorator.SetName(link, "UniqueTestName");
                
                var retrievedLink = decorator.GetByName("UniqueTestName");
                
                Assert.Equal(link, retrievedLink);
            });
        }

        [Fact]
        public void NamedTypesDecorator_CanRemoveNames()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                var link = decorator.GetOrCreate(70u, 80u);
                decorator.SetName(link, "TemporaryName");
                
                var nameBeforeRemoval = decorator.GetName(link);
                Assert.Equal("TemporaryName", nameBeforeRemoval);
                
                decorator.RemoveName(link);
                
                var nameAfterRemoval = decorator.GetName(link);
                Assert.Null(nameAfterRemoval);
                
                var linkByName = decorator.GetByName("TemporaryName");
                Assert.Equal(links.Constants.Null, linkByName);
            });
        }

        [Fact]
        public void NamedTypesDecorator_CanOverwriteNames()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                var link = decorator.GetOrCreate(90u, 100u);
                decorator.SetName(link, "FirstName");
                
                var firstRetrievedName = decorator.GetName(link);
                Assert.Equal("FirstName", firstRetrievedName);
                
                decorator.SetName(link, "SecondName");
                
                var secondRetrievedName = decorator.GetName(link);
                Assert.Equal("SecondName", secondRetrievedName);
                
                var linkByFirstName = decorator.GetByName("FirstName");
                Assert.Equal(links.Constants.Null, linkByFirstName);
                
                var linkBySecondName = decorator.GetByName("SecondName");
                Assert.Equal(link, linkBySecondName);
            });
        }

        [Fact]
        public void NamedTypesDecorator_DeleteRemovesAssociatedNames()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                var link = decorator.GetOrCreate(110u, 120u);
                decorator.SetName(link, "LinkToDelete");
                
                var nameBeforeDeletion = decorator.GetName(link);
                Assert.Equal("LinkToDelete", nameBeforeDeletion);
                
                decorator.Delete(new uint[] { link }, null);
                
                var linkByName = decorator.GetByName("LinkToDelete");
                Assert.Equal(links.Constants.Null, linkByName);
            });
        }

        [Fact]
        public void NamedTypesDecorator_HandlesNonexistentNames()
        {
            RunTestWithLinks(links =>
            {
                var decorator = new NamedTypesDecorator<uint>(links, _tempNamesDbPath);
                
                var linkByNonexistentName = decorator.GetByName("NonexistentName");
                Assert.Equal(links.Constants.Null, linkByNonexistentName);
                
                var nameOfNonexistentLink = decorator.GetName(999999u);
                Assert.Null(nameOfNonexistentLink);
            });
        }
    }
}