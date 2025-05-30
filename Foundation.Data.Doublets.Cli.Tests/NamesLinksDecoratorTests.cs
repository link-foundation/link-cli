using Xunit;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Foundation.Data.Doublets.Cli;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class NamesLinksDecoratorTests
    {
        [Fact]
        public void CanConstructNamesLinksDecorator()
        {
            // Arrange
            var tempDbFile = System.IO.Path.GetTempFileName();
            using var links = new UnitedMemoryLinks<ulong>(tempDbFile);

            // Act
            var decorator = new NamesLinksDecorator<ulong>(links);

            // Assert
            Assert.NotNull(decorator);
        }
    }
}
