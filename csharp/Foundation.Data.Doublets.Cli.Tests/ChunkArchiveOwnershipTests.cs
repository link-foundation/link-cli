using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class ChunkArchiveOwnershipTests
{
    [Fact(Timeout = 15000)]
    public async Task CompetingArchiveWritersPublishOnlyOneCompleteOwner()
    {
        var directory = Directory.CreateTempSubdirectory("archive-race-").FullName;
        var path = Path.Combine(directory, "archive.log");
        try
        {
            var attempts = Enumerable.Range(0, 8).Select(owner => Task.Run(() =>
            {
                try
                {
                    ChunkArchive.Write(path, [new string((char)('a' + owner), 1024)]);
                    return owner;
                }
                catch (IOException) { return -1; }
            })).ToArray();
            var results = await Task.WhenAll(attempts).WaitAsync(TimeSpan.FromSeconds(10));
            var winner = Assert.Single(results, owner => owner >= 0);
            Assert.Equal([new string((char)('a' + winner), 1024)], File.ReadAllLines(path));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory(Timeout = 20000)]
    [InlineData(CommitMode.Sync)]
    [InlineData(CommitMode.Async)]
    public async Task FailedOptionalArchiveKeepsHistoryForRetryAndRepeatedRecovery(CommitMode mode)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("archive-retry-").FullName;
        var dataPath = Path.Combine(directory, "data");
        var logPath = Path.Combine(directory, "journal");
        var archive = Path.Combine(directory, "archive");
        try
        {
            File.WriteAllText(archive, "existing entry");
            uint row;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            {
                row = data.CreateAndUpdate(11u, 12u);
                data.Flush();
                using var tx = new TransactionsDecorator(data, log, new LogRetentionPolicy.Chunked(1, archive), mode);
                using (var first = tx.BeginTransaction())
                {
                    tx.Update(new[] { row }, new[] { row, 31u, 32u }, null);
                    first.Commit();
                }
                tx.Shutdown();
                Assert.Single(tx.Log);
                Assert.False(tx.HasOpenTransaction);
                Assert.Equal(31u, tx.GetSource(row));
                Assert.Equal("existing entry", File.ReadAllText(archive));
                File.Delete(archive);
                tx.CommitMode = CommitMode.Sync;
                tx.Update(new[] { row }, new[] { row, 51u, 52u }, null);
                var original = Assert.Single(Directory.GetFiles(archive));
                var bytes = File.ReadAllBytes(original);
                Assert.True(Transition<uint>.TryParse(Assert.Single(File.ReadAllLines(original)), out var retained));
                Assert.Equal(31u, retained.After.Source);
                tx.Update(new[] { row }, new[] { row, 61u, 62u }, null);
                Assert.Equal(bytes, File.ReadAllBytes(original));
                Assert.Equal(2, Directory.GetFiles(archive).Length);
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(dataPath);
                using var log = new NamedTypesDecorator<uint>(logPath);
                using var tx = new TransactionsDecorator(data, log);
                Assert.Equal(61u, tx.GetSource(row));
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData("file")]
    [InlineData("directory")]
    [InlineData("symlink")]
    [InlineData("dangling-symlink")]
    public void ArchiveCollisionPreservesTheExistingEntry(string kind)
    {
        // Windows symlink privileges are not assumed by the test runner.
        if (kind.Contains("symlink") && !OperatingSystem.IsLinux()) return;
        var directory = Path.Combine(Path.GetTempPath(), "archive-owner-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, "archive.log");
        var target = Path.Combine(directory, "original.log");
        try
        {
            File.WriteAllText(target, "original acknowledged archive\n");
            switch (kind)
            {
                case "file": File.Copy(target, path); break;
                case "directory": Directory.CreateDirectory(path); break;
                case "symlink": File.CreateSymbolicLink(path, target); break;
                case "dangling-symlink": File.CreateSymbolicLink(path, target + ".missing"); break;
            }
            var enumerated = false;
            IEnumerable<string> NewRecords()
            {
                enumerated = true;
                yield return "new owner's archive";
            }
            var error = Record.Exception(() => ChunkArchive.Write(path, NewRecords()));
            Assert.True(error is IOException or UnauthorizedAccessException, error?.ToString() ?? "Existing archive was overwritten.");
            Assert.False(enumerated, "A rejected archive must not consume its source records.");
            Assert.Equal("original acknowledged archive\n", File.ReadAllText(target));
            if (kind == "file") Assert.Equal(File.ReadAllText(target), File.ReadAllText(path));
            if (kind == "directory") Assert.Empty(Directory.GetFileSystemEntries(path));
            if (kind.Contains("symlink"))
            {
                Assert.Equal(kind == "symlink" ? target : target + ".missing", new FileInfo(path).LinkTarget);
                Assert.False(File.Exists(target + ".missing"));
            }
            var following = Path.Combine(directory, "following.log");
            ChunkArchive.Write(following, ["following acknowledged archive"]);
            Assert.Equal(["following acknowledged archive"], File.ReadAllLines(following));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
