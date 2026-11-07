using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineSessionTests
{
    [Fact]
    public void ExplicitAddressesNamesAndDeletionRemainPrivateUntilCommit()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        Assert.Throws<ArgumentOutOfRangeException>(() => tx.EnsureCreated(0));
        Assert.Equal(42u, tx.EnsureCreated(42));
        Assert.Equal(42u, tx.EnsureCreated(42));
        Assert.Equal(43u, tx.CreateLink(42, 0));
        tx.SetName(42, "before");
        Assert.Equal("before", tx.GetName(42));
        tx.RemoveName(42);
        Assert.Null(tx.GetName(42));
        Assert.Equal(0u, tx.GetByName("before"));
        Assert.Throws<ArgumentNullException>(() => tx.SetName(42, null!));
        Assert.Throws<KeyNotFoundException>(() => tx.SetName(99, "missing"));
        Assert.Throws<KeyNotFoundException>(() => tx.RemoveName(99));
        Assert.Throws<KeyNotFoundException>(() => tx.UpdateLink(99, 0, 0));
        Assert.Throws<KeyNotFoundException>(() => tx.DeleteLink(99));
        tx.DeleteLink(42);
        Assert.Null(tx.GetLink(42));
        Assert.Empty(engine.Snapshot().Rows);
        tx.Commit();
        Assert.Equal(43u, Assert.Single(engine.Snapshot().Rows).Key);
    }

    [Fact]
    public void RestrictionsAndCallbacksUseTheStandardLinksContract()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        tx.EnsureCreated(1); tx.CreateLink(1, 9); tx.CreateLink(8, 1);
        uint any = tx.Constants.Any;
        Assert.Equal(3u, tx.Count(Array.Empty<uint>()));
        Assert.Equal(1u, tx.Count(new uint[] { 1 }));
        Assert.Equal(2u, tx.Count(new[] { any, 1u }));
        Assert.Equal(1u, tx.Count(new[] { 2u, 9u }));
        Assert.Equal(0u, tx.Count(new[] { 2u, 8u }));
        Assert.Equal(1u, tx.Count(new[] { any, 1u, 9u }));
        Assert.Equal(0u, tx.Count(new[] { any, 1u, 8u }));
        Assert.Throws<ArgumentException>(() => tx.Count(new uint[4]));
        int calls = 0;
        Assert.Equal(tx.Constants.Break, tx.Each(null, _ => { calls++; return tx.Constants.Break; }));
        Assert.Equal(1, calls);
        Assert.Equal(tx.Constants.Continue, tx.Each(null, null));
        Assert.Equal(tx.Constants.Continue, tx.Each(null, _ => tx.Constants.Continue));
        Assert.Equal(tx.Constants.Break, tx.Create(null, (before, after) =>
        { Assert.Null(before); Assert.Equal(4u, after![0]); return tx.Constants.Break; }));
        Assert.Equal(tx.Constants.Break, tx.Update(new[] { 4u }, new[] { 4u, 2u, 3u }, (before, after) =>
        { Assert.Equal(0u, before![1]); Assert.Equal(2u, after![1]); return tx.Constants.Break; }));
        Assert.Equal(tx.Constants.Break, tx.Delete(new[] { 4u }, (before, after) =>
        { Assert.Equal(4u, before![0]); Assert.Null(after); return tx.Constants.Break; }));
        Assert.Throws<ArgumentException>(() => tx.Update(null, new uint[3], null));
        Assert.Throws<ArgumentException>(() => tx.Update(new uint[1], null, null));
        Assert.Throws<ArgumentException>(() => tx.Delete(null, null));
        Assert.Equal(tx.Constants.Continue, tx.Update(new[] { 2u }, new[] { 2u, 0u, 0u }, null));
        Assert.Equal(tx.Constants.Continue, tx.Delete(new[] { 2u }, null));
        tx.Commit();
        Assert.Equal(2, engine.Snapshot().Rows.Count);
    }

    [Fact]
    public void AddressExhaustionAndLogCallbackFailureDoNotPublishPartialState()
    {
        var engine = new LinksEngine<byte>(new MemoryCommitLog());
        using (var tx = engine.Begin())
        {
            tx.EnsureCreated(byte.MaxValue);
            Assert.Throws<OverflowException>(() => tx.CreateLink(0, 0));
            tx.Commit();
        }
        Assert.Single(engine.Snapshot().Rows);
        Assert.Throws<KeyNotFoundException>(() => engine.SnapshotAt(0));
        Assert.Same(engine.Snapshot(), engine.SnapshotAt(1));
        Assert.Throws<IOException>(() => engine.WithLog<int>(_ => throw new IOException("lost log")));
        Assert.Throws<InvalidOperationException>(() => engine.Begin());
        var wide = new LinksEngine<UInt128>(new MemoryCommitLog());
        using var wideTx = wide.Begin();
        Assert.Throws<OverflowException>(() => wideTx.EnsureCreated(UInt128.MaxValue));
        Assert.Empty(wide.Snapshot().Rows);
    }

    [Fact]
    public void FileAdapterRepairsLongAndEmptyTailsAndReleasesFailedOpenLocks()
    {
        var directory = Path.Combine(Path.GetTempPath(), "engine-files-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, "journal");
        try
        {
            Directory.CreateDirectory(path);
            var openError = Record.Exception(() => new FileCommitLog(path));
            Assert.True(openError is IOException or UnauthorizedAccessException,
                $"Expected the operating system to reject opening a directory: {openError}");
            Directory.Delete(path);
            File.WriteAllText(path, new string('x', 20000));
            using (var empty = new FileCommitLog(path))
            {
                Assert.Empty(empty.Read());
                Assert.Throws<ArgumentException>(() => empty.Append("line\nbreak"));
                Assert.Throws<ArgumentException>(() => empty.Append("line\rbreak"));
                empty.Append("committed"); empty.Flush();
            }
            File.AppendAllText(path, new string('x', 20000));
            using (var repaired = new FileCommitLog(path)) Assert.Equal(new[] { "committed" }, repaired.Read());
            File.WriteAllBytes(path, new byte[] { 255, 10 });
            using var invalid = new FileCommitLog(path);
            Assert.Throws<System.Text.DecoderFallbackException>(() => invalid.Read());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
