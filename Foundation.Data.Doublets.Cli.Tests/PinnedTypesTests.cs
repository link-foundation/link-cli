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
    public class PinnedTypesTests
    {
        [Fact]
        public void Should_Create_And_Iterate_Over_Types()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var initialSource = 1UL;
                var numberOfTypes = 3;

                var pinnedTypes = new PinnedTypes<ulong>(links, initialSource, numberOfTypes);

                var allLinks = links.All();

                // Act
                var result = new List<ulong>();
                foreach (var type in pinnedTypes)
                {
                    result.Add(type);
                }

                // Assert
                Assert.Equal(numberOfTypes, result.Count);
                Assert.Equal(new ulong[] { 1, 2, 3 }, result); // Updated expected addresses
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Validate_Existing_Links()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var initialSource = 1UL;
                var numberOfTypes = 2;

                // Pre-create links, including the first link
                links.GetOrCreate(initialSource, 1UL); // First link
                links.GetOrCreate(initialSource, 2UL);
                links.GetOrCreate(initialSource, 3UL);

                var pinnedTypes = new PinnedTypes<ulong>(links, initialSource, numberOfTypes);

                var allLinks = links.All();
                
                // Act
                var result = new List<ulong>();
                foreach (var type in pinnedTypes)
                {
                    result.Add(type);
                }

                // Assert
                Assert.Equal(numberOfTypes, result.Count);
                Assert.Equal(new ulong[] { 1, 2 }, result); // Expected addresses
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Throw_Exception_For_Invalid_Link_Structure()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var initialSource = 1UL;
                var numberOfTypes = 3;

                // Create valid links
                links.GetOrCreate(initialSource, 1UL); // Valid link
                links.GetOrCreate(initialSource, 2UL); // Valid link

                // Create an invalid link
                links.GetOrCreate(initialSource, 0UL); // Invalid link with unexpected address

                var pinnedTypes = new PinnedTypes<ulong>(links, initialSource, numberOfTypes);

                var allLinks = links.All();
                
                // Act & Assert
                var exception = Assert.Throws<InvalidOperationException>(() =>
                {
                    var result = new List<ulong>();
                    foreach (var type in pinnedTypes.Take(numberOfTypes))
                    {
                        result.Add(type);
                    }
                });

                Assert.Contains("Unexpected link found at address", exception.Message);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Reset_Enumerator()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var initialSource = 1UL;
                var numberOfTypes = 2;

                var pinnedTypes = new PinnedTypes<ulong>(links, initialSource, numberOfTypes);
                var enumerator = pinnedTypes.GetEnumerator();

                var allLinks = links.All();
                
                // Act
                enumerator.MoveNext();
                var first = enumerator.Current;

                enumerator.Reset();
                enumerator.MoveNext();
                var resetFirst = enumerator.Current;

                // Assert
                Assert.Equal(first, resetFirst);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Validate_Existing_Links_With_Ulong()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var initialSource = 1UL;
                var numberOfTypes = 2;

                // Pre-create links
                links.GetOrCreate(initialSource, 1UL); // First link
                links.GetOrCreate(initialSource, 2UL);
                links.GetOrCreate(initialSource, 3UL);

                var pinnedTypes = new PinnedTypes<ulong>(links, initialSource, numberOfTypes);

                var allLinks = links.All();
                
                // Act
                var result = new List<ulong>();
                foreach (var type in pinnedTypes)
                {
                    result.Add(type);
                }

                // Assert
                Assert.Equal(numberOfTypes, result.Count);
                Assert.Equal(new ulong[] { 1, 2 }, result); // Expected addresses
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Create_And_Iterate_Over_Types_With_RealDataStore()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var initialSource = 1UL;
                var numberOfTypes = 3;

                var pinnedTypes = new PinnedTypes<ulong>(links, initialSource, numberOfTypes);

                var allLinks = links.All();
                
                // Act
                var result = new List<ulong>();
                foreach (var type in pinnedTypes)
                {
                    result.Add(type);
                }

                // Assert
                Assert.Equal(numberOfTypes, result.Count);
                Assert.Equal(new ulong[] { 1, 2, 3 }, result); // Expected addresses
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void Should_Destructure_PinnedTypes()
        {
            // Arrange
            var tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<ulong>(tempDbFile);
                var initialSource = 1UL;
                var numberOfTypes = 3;

                // Pre-create links
                links.GetOrCreate(initialSource, 1UL);
                links.GetOrCreate(initialSource, 2UL);
                links.GetOrCreate(initialSource, 3UL);

                var pinnedTypes = new PinnedTypes<ulong>(links, initialSource, numberOfTypes);

                // Act
                var (type1, type2, type3) = pinnedTypes;

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
    }
}