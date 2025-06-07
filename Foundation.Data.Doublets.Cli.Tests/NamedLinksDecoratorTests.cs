using Xunit;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Foundation.Data.Doublets.Cli;
using System.Numerics;
using System.IO;
using System.Collections.Generic;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class NamedLinksDecoratorTests
    {
        [Fact]
        public void CanConstructNamedLinksDecorator()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();

            // Act
            var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);
            var namesDatabaseFilename = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);

            // Assert
            Assert.NotNull(decorator);

            // Clean up
            if (File.Exists(tempDbFile))
            {
                File.Delete(tempDbFile);
            }
            if (File.Exists(namesDatabaseFilename))
            {
                File.Delete(namesDatabaseFilename);
            }
        }

        [Theory]
        [InlineData("/tmp/test.db", "/tmp/test.names.links")]
        [InlineData("test.db", "test.names.links")]
        [InlineData("a.b.c", "a.b.names.links")]
        public void MakeNamesDatabaseFilename_CorrectlyGeneratesFilename(string dbFilename, string expected)
        {
            var result = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(dbFilename);
            Assert.Equal(expected, result);
        }

        [Fact]
        public void SetNameAndGetName_ShouldReturnSameName()
        {
            var tempDbFile = Path.GetTempFileName();
            var expectedNamesDb = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);
                var link = decorator.GetOrCreate(10u, 20u);
                string name = "testName";
                decorator.SetName(link, name);
                var returnedName = decorator.GetName(link);
                Assert.Equal(name, returnedName);
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(expectedNamesDb)) File.Delete(expectedNamesDb);
            }
        }

        [Fact]
        public void SetName_OverwriteOldName()
        {
            var tempDbFile = Path.GetTempFileName();
            var expectedNamesDb = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);
                var link = decorator.GetOrCreate(1u, 2u);
                string firstName = "first";
                string secondName = "second";
                decorator.SetName(link, firstName);
                Assert.Equal(firstName, decorator.GetName(link));
                decorator.SetName(link, secondName);
                Assert.Equal(secondName, decorator.GetName(link));
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(expectedNamesDb)) File.Delete(expectedNamesDb);
            }
        }

        [Fact]
        public void RemoveName_ShouldReturnNullAfterRemoval()
        {
            var tempDbFile = Path.GetTempFileName();
            var expectedNamesDb = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);
                var link = decorator.GetOrCreate(5u, 6u);
                string name = "name";
                decorator.SetName(link, name);
                Assert.Equal(name, decorator.GetName(link));
                decorator.RemoveName(link);
                Assert.Null(decorator.GetName(link));
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(expectedNamesDb)) File.Delete(expectedNamesDb);
            }
        }

        [Fact]
        public void RemoveName_NonExistent_DoesNotThrow()
        {
            var tempDbFile = Path.GetTempFileName();
            var expectedNamesDb = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);
                var link = decorator.GetOrCreate(7u, 8u);
                decorator.RemoveName(link);
                Assert.Null(decorator.GetName(link));
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(expectedNamesDb)) File.Delete(expectedNamesDb);
            }
        }

        [Fact]
        public void AfterCreation_SetNameAndGetName_ShouldWork()
        {
            var tempDbFile = Path.GetTempFileName();
            var expectedNamesDb = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);
                var link = decorator.GetOrCreate(10u, 20u);
                string name = "myLinkName";
                decorator.SetName(link, name);
                Assert.Equal(name, decorator.GetName(link));
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(expectedNamesDb)) File.Delete(expectedNamesDb);
            }
        }

        [Fact]
        public void DeleteLink_RemovesNameAutomatically()
        {
            var tempDbFile = Path.GetTempFileName();
            var expectedNamesDb = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);
                var source = 30u;
                var target = 40u;
                var link = decorator.GetOrCreate(source, target);
                string name = "toDelete";
                decorator.SetName(link, name);
                Assert.Equal(name, decorator.GetName(link));
                var restriction = new List<uint> { source, target };
                decorator.Delete(restriction, null);
                Assert.Null(decorator.GetName(link));
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(expectedNamesDb)) File.Delete(expectedNamesDb);
            }
        }
    }
}
