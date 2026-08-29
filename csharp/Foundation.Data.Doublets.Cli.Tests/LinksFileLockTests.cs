namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    /// <summary>
    /// Covers the multi-process helpers a consumer needs when several
    /// processes open the same links database.
    /// </summary>
    public class LinksFileLockTests
    {
        [Fact]
        public void LockFilePathIsTheDatabaseSidecar()
        {
            Assert.Equal(
              Path.Combine("data", "db.links.lock"),
              LinksFileLock.LockFilePath(Path.Combine("data", "db.links")));
        }

        [Fact]
        public void AnExclusiveLockExcludesEveryOtherHolder()
        {
            var lockPath = NewLockPath();
            using var writer = LinksFileLock.Acquire(lockPath, LockMode.Exclusive, TimeSpan.FromSeconds(5));

            Assert.Null(LinksFileLock.TryAcquire(lockPath, LockMode.Exclusive));
            Assert.Null(LinksFileLock.TryAcquire(lockPath, LockMode.Shared));
            Assert.Equal(LockMode.Exclusive, writer.Mode);
            Assert.Equal(lockPath, writer.Path);
        }

        [Fact]
        public void SharedLocksCoexistButStillExcludeWriters()
        {
            var lockPath = NewLockPath();
            using var firstReader = LinksFileLock.Acquire(lockPath, LockMode.Shared, TimeSpan.FromSeconds(5));
            using var secondReader = LinksFileLock.TryAcquire(lockPath, LockMode.Shared);

            Assert.NotNull(secondReader);
            Assert.Null(LinksFileLock.TryAcquire(lockPath, LockMode.Exclusive));
        }

        [Fact]
        public void ReleasingALockLetsTheNextHolderIn()
        {
            var lockPath = NewLockPath();
            using (LinksFileLock.Acquire(lockPath, LockMode.Exclusive, TimeSpan.FromSeconds(5)))
            {
                Assert.Null(LinksFileLock.TryAcquire(lockPath, LockMode.Exclusive));
            }

            using var next = LinksFileLock.TryAcquire(lockPath, LockMode.Exclusive);
            Assert.NotNull(next);
        }

        [Fact]
        public void AcquireGivesUpAfterItsTimeout()
        {
            var lockPath = NewLockPath();
            using var held = LinksFileLock.Acquire(lockPath, LockMode.Exclusive, TimeSpan.FromSeconds(5));

            Assert.Throws<TimeoutException>(
              () => LinksFileLock.Acquire(lockPath, LockMode.Exclusive, TimeSpan.FromMilliseconds(50)));
        }

        [Fact]
        public void AMissingDatabaseHasTheDefaultRevision()
        {
            var missing = Path.Combine(NewDirectory(), "absent.links");
            Assert.Equal(default, StorageRevision.Of(missing));
            Assert.False(default(StorageRevision).HasChanged(missing));
        }

        [Fact]
        public void ARevisionDetectsAWriteByAnotherHolder()
        {
            var file = Path.Combine(NewDirectory(), "db.links");
            File.WriteAllText(file, "first");
            var revision = StorageRevision.Of(file);

            Assert.False(revision.HasChanged(file));

            File.WriteAllText(file, "first and second");

            Assert.True(revision.HasChanged(file), "Appending to the database must change its revision.");
            Assert.NotEqual(revision, StorageRevision.Of(file));
        }

        private static string NewDirectory()
        {
            var directory = Path.Combine(Path.GetTempPath(), "clink-lock-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(directory);
            return directory;
        }

        private static string NewLockPath() => LinksFileLock.LockFilePath(Path.Combine(NewDirectory(), "db.links"));
    }
}
