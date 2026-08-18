using System;
using System.IO;
using System.Collections.Generic;
using Xunit;
using Foundation.Data.Doublets.Cli;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class SimpleLinksDecoratorTests
    {
        [Fact]
        public void CanConstructSimpleLinksDecorator()
        {
            var tempDbFile = Path.GetTempFileName();
            var namesDbFile = SimpleLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                using var decorator = new SimpleLinksDecorator<uint>(tempDbFile);
                Assert.NotNull(decorator);
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(namesDbFile)) File.Delete(namesDbFile);
            }
        }

        [Fact]
        public void Delete_WithRestriction_DoesNotThrow()
        {
            var tempDbFile = Path.GetTempFileName();
            var namesDbFile = SimpleLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                using var decorator = new SimpleLinksDecorator<uint>(tempDbFile);
                var source = 1u;
                var target = 2u;
                // create a link so there is something to delete
                var link = decorator.GetOrCreate(source, target);
                // build a restriction triple: [index, source, target]
                var restriction = new List<uint> { 0u, 0u, 0u };
                restriction[decorator.Constants.IndexPart] = link;
                restriction[decorator.Constants.SourcePart] = source;
                restriction[decorator.Constants.TargetPart] = target;
                // invoking Delete should not throw
                decorator.Delete(restriction, null);
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(namesDbFile)) File.Delete(namesDbFile);
            }
        }

        [Fact]
        public void DeleteAfterGetOrCreate_DoesNotThrow()
        {
            var tempDbFile = Path.GetTempFileName();
            var namesDbFile = SimpleLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                using var decorator = new SimpleLinksDecorator<uint>(tempDbFile);
                var source = 1u;
                var target = 1u;
                var link = decorator.GetOrCreate(source, target);
                var restriction = new Link<uint>(link, source, target);
                decorator.Delete(restriction, null);
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(namesDbFile)) File.Delete(namesDbFile);
            }
        }

        [Fact]
        public void DeleteAfterGetOrCreate_DoesNotThrow_WithTracing()
        {
            var tempDbFile = Path.GetTempFileName();
            var namesDbFile = SimpleLinksDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try {
                using var decorator = new SimpleLinksDecorator<uint>(tempDbFile, true);
                var source = 1u;
                var target = 1u;
                var link = decorator.GetOrCreate(source, target);
                var restriction = new Link<uint>(link, source, target);
                decorator.Delete(restriction, null);
            }
            finally
            {
                if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
                if (File.Exists(namesDbFile)) File.Delete(namesDbFile);
            }
        }
    }
} 