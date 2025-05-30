using Xunit;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Foundation.Data.Doublets.Cli;
using System.Numerics;
using System.IO;

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
            var decorator = new NamedLinksDecorator<uint>(tempDbFile);
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
    }
}
