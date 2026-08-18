using System;
using System.IO;
using Xunit;
using Foundation.Data.Doublets.Cli;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests
{
    /// <summary>
    /// Regression tests for the Windows CI failures tracked by issue #96.
    /// </summary>
    /// <remarks>
    /// Every decorator opens memory-mapped files for the data database and for the names database.
    /// POSIX lets a still-mapped file be unlinked, so a leaked handle is invisible on Linux and macOS;
    /// Windows uses mandatory locking and fails the delete with <see cref="IOException"/>. That is what
    /// produced 226 IOExceptions across 114 failing tests on windows-latest, which the CI workflow then
    /// hid behind `continue-on-error`. These tests pin the contract that makes the delete safe
    /// everywhere: the decorators are disposable, and disposing them releases both databases.
    /// </remarks>
    public class DecoratorDisposalTests
    {
        [Fact]
        public void NamedLinksDecorator_IsDisposable() => Assert.True(typeof(IDisposable).IsAssignableFrom(typeof(NamedLinksDecorator<uint>)));

        [Fact]
        public void NamedTypesDecorator_IsDisposable() => Assert.True(typeof(IDisposable).IsAssignableFrom(typeof(NamedTypesDecorator<uint>)));

        [Fact]
        public void SimpleLinksDecorator_IsDisposable() => Assert.True(typeof(IDisposable).IsAssignableFrom(typeof(SimpleLinksDecorator<uint>)));

        [Fact]
        public void NamedLinksDecorator_DatabasesCanBeDeletedAfterDispose()
            => AssertDatabasesCanBeDeletedAfterDispose(
                NamedLinksDecorator<uint>.MakeNamesDatabaseFilename,
                databaseFilename => new NamedLinksDecorator<uint>(databaseFilename));

        [Fact]
        public void NamedTypesDecorator_DatabasesCanBeDeletedAfterDispose()
            => AssertDatabasesCanBeDeletedAfterDispose(
                NamedTypesDecorator<uint>.MakeNamesDatabaseFilename,
                databaseFilename => new NamedTypesDecorator<uint>(databaseFilename));

        [Fact]
        public void SimpleLinksDecorator_DatabasesCanBeDeletedAfterDispose()
            => AssertDatabasesCanBeDeletedAfterDispose(
                SimpleLinksDecorator<uint>.MakeNamesDatabaseFilename,
                databaseFilename => new SimpleLinksDecorator<uint>(databaseFilename));

        [Fact]
        public void Dispose_IsIdempotent()
        {
            var databaseFilename = Path.GetTempFileName();
            var namesDatabaseFilename = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(databaseFilename);
            try
            {
                var decorator = new NamedTypesDecorator<uint>(databaseFilename);
                decorator.GetOrCreate(1u, 1u);

                decorator.Dispose();
                decorator.Dispose();
            }
            finally
            {
                Delete(databaseFilename);
                Delete(namesDatabaseFilename);
            }
        }

        private static void AssertDatabasesCanBeDeletedAfterDispose(
            Func<string, string> makeNamesDatabaseFilename,
            Func<string, ILinks<uint>> createDecorator)
        {
            var databaseFilename = Path.GetTempFileName();
            var namesDatabaseFilename = makeNamesDatabaseFilename(databaseFilename);
            try
            {
                var decorator = createDecorator(databaseFilename);
                // Force both databases to be materialised before the handles are released.
                decorator.GetOrCreate(1u, 1u);
                ((IDisposable)decorator).Dispose();

                // Before the fix this threw IOException on Windows because the handles were still open.
                File.Delete(databaseFilename);
                File.Delete(namesDatabaseFilename);

                Assert.False(File.Exists(databaseFilename));
                Assert.False(File.Exists(namesDatabaseFilename));
            }
            finally
            {
                Delete(databaseFilename);
                Delete(namesDatabaseFilename);
            }
        }

        private static void Delete(string path)
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
    }
}
