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

            var namesDatabaseFilename = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                // Act
                using var decorator = new NamedLinksDecorator<uint>(tempDbFile, true);

                // Assert
                Assert.NotNull(decorator);
            }
            finally
            {
                // Clean up: the decorator is disposed by the `using` above, so the memory-mapped
                // files are unlocked and can be deleted on Windows as well.
                if (File.Exists(tempDbFile))
                {
                    File.Delete(tempDbFile);
                }
                if (File.Exists(namesDatabaseFilename))
                {
                    File.Delete(namesDatabaseFilename);
                }
            }
        }

        // Asserted as "directory of the input" + "expected file name" rather than as one hard-coded
        // string: the implementation builds the result with Path.Combine, which emits '\\' on Windows,
        // so an expectation such as "/tmp/test.names.links" passes on Linux and macOS but fails on
        // Windows for a purely cosmetic reason.
        [Theory]
        [InlineData("/tmp/test.db", "test.names.links")]
        [InlineData("test.db", "test.names.links")]
        [InlineData("a.b.c", "a.b.names.links")]
        public void MakeNamesDatabaseFilename_CorrectlyGeneratesFilename(string dbFilename, string expectedFileName)
        {
            var result = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(dbFilename);

            Assert.Equal(expectedFileName, Path.GetFileName(result));
            Assert.Equal(Path.GetDirectoryName(dbFilename), Path.GetDirectoryName(result));
        }

        // All three decorators duplicate MakeNamesDatabaseFilename; they must agree on every platform.
        [Theory]
        [InlineData("/tmp/test.db")]
        [InlineData("test.db")]
        [InlineData("a.b.c")]
        public void MakeNamesDatabaseFilename_IsConsistentAcrossDecorators(string dbFilename)
        {
            var expected = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(dbFilename);

            Assert.Equal(expected, NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dbFilename));
            Assert.Equal(expected, SimpleLinksDecorator<uint>.MakeNamesDatabaseFilename(dbFilename));
        }

        [Fact]
        public void SetNameAndGetName_ShouldReturnSameName()
        {
            var tempDbFile = Path.GetTempFileName();
            var expectedNamesDb = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                using var decorator = new NamedLinksDecorator<uint>(tempDbFile, false);
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
                using var decorator = new NamedLinksDecorator<uint>(tempDbFile, false);
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
                using var decorator = new NamedLinksDecorator<uint>(tempDbFile, false);
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
                using var decorator = new NamedLinksDecorator<uint>(tempDbFile, false);
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
                using var decorator = new NamedLinksDecorator<uint>(tempDbFile, false);
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
            var namesDatabaseFilename = NamedLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                using var decorator = new NamedLinksDecorator<uint>(tempDbFile, false);
                var source = 1u;
                var target = 1u;
                var link = decorator.GetOrCreate(source, target);
                string name = "toDelete";
                decorator.SetName(link, name);
                Assert.Equal(name, decorator.GetName(link));
                var restriction = new Link<uint>(link, source, target);
                decorator.Delete(link, null);
                Assert.Null(decorator.GetName(link));
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(namesDatabaseFilename)) File.Delete(namesDatabaseFilename);
            }
        }
    }
}
