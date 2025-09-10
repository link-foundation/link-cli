using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Delegates;
using Xunit;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class PinnedTypesDecoratorTests
    {
        [Fact]
        public void Should_Implement_Both_ILinks_And_IPinnedTypes()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                
                // Act
                var decorator = new PinnedTypesDecorator<ulong>(links);
                
                // Assert - Should implement both interfaces
                Assert.IsAssignableFrom<ILinks<ulong>>(decorator);
                Assert.IsAssignableFrom<IPinnedTypes<ulong>>(decorator);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Enumerate_PinnedTypes()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var decorator = new PinnedTypesDecorator<ulong>(links);
                var numberOfTypes = 3;

                // Act
                var result = new List<ulong>();
                foreach (var type in decorator.Take(numberOfTypes))
                {
                    result.Add(type);
                }

                // Assert
                Assert.Equal(numberOfTypes, result.Count);
                Assert.Equal(new ulong[] { 1, 2, 3 }, result);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Support_Deconstruction()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var decorator = new PinnedTypesDecorator<ulong>(links);
                var initialSource = 1UL;

                // Pre-create links to ensure they exist
                links.GetOrCreate(initialSource, 1UL);
                links.GetOrCreate(initialSource, 2UL);
                links.GetOrCreate(initialSource, 3UL);

                // Act
                var (type1, type2, type3) = decorator;

                // Assert
                Assert.Equal(1UL, type1);
                Assert.Equal(2UL, type2);
                Assert.Equal(3UL, type3);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Work_As_ILinks_Decorator()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var baseLinks = new UnitedMemoryLinks<ulong>(tempDbFile);
                var decorator = new PinnedTypesDecorator<ulong>(baseLinks);
                
                // Act & Assert - Test that it still works as a decorator and properly implements both interfaces
                Assert.NotNull(decorator);
                Assert.IsAssignableFrom<ILinks<ulong>>(decorator);
                Assert.IsAssignableFrom<IPinnedTypes<ulong>>(decorator);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }
    }
}