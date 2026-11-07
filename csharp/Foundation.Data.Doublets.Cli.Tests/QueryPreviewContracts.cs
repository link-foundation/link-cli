using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed class QueryPreviewContracts
{
    [Fact]
    public void PreviewPreservesOrdinaryMatchingNamesCallbacksAndArgumentGuards()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        tx.CreateLink(1, 1); tx.CreateLink(1, 0); tx.CreateLink(0, 3); tx.SetName(1, "original");
        tx.ReserveQueryAllocations(preview =>
        {
            var any = preview.Constants.Any;
            Assert.Equal(3u, preview.Count(null));
            Assert.Equal(3u, preview.Count([]));
            Assert.Equal(1u, preview.Count([2]));
            Assert.Equal(0u, preview.Count([99]));
            Assert.Equal(2u, preview.Count([any, 1]));
            Assert.Equal(1u, preview.Count([2, 0]));
            Assert.Equal(0u, preview.Count([2, 7]));
            Assert.Equal(0u, preview.Count([99, any]));
            Assert.Equal(1u, preview.Count([1, any]));
            Assert.Equal(3u, preview.Count([any, any]));
            Assert.Equal(1u, preview.Count([any, any, 0]));
            Assert.Equal(1u, preview.Count([any, 0, any]));
            Assert.Equal(1u, preview.Count([3, 0, 3]));
            Assert.Throws<ArgumentException>(() => preview.Count([any, any, any, any]));
            Assert.Equal(preview.Constants.Continue, preview.Each([1], _ => preview.Constants.Continue));
            Assert.Equal(preview.Constants.Break, preview.Each([2], _ => preview.Constants.Break));
            Assert.Equal(preview.Constants.Continue, preview.Each([3], null));
            Assert.Equal(preview.Constants.Continue, preview.Each([99], _ => throw new InvalidOperationException()));
            Assert.Equal("original", preview.GetName(1));
            Assert.Equal(1u, preview.GetByName("original"));
            Assert.Equal(0u, preview.GetByName("absent"));
            Assert.Null(preview.GetName(99));
            Assert.True(preview.TryEnsureCreated(1));
            Assert.Equal("original", preview.GetName(1));
            Assert.Throws<InvalidOperationException>(() => preview.TryEnsureCreated(0));
            Assert.Throws<ArgumentException>(() => preview.Update(null, [2, 0, 1], null));
            Assert.Throws<ArgumentException>(() => preview.Update([], [2, 0, 1], null));
            Assert.Throws<ArgumentException>(() => preview.Update([2], [2, 0], null));
            Assert.Throws<KeyNotFoundException>(() => preview.Update([99], [99, 0, 0], null));
            IList<uint>? observed = null;
            Assert.Equal(preview.Constants.Break, preview.Update([2], [2, 0, 1], (before, after) =>
            {
                Assert.Equal(new uint[] { 2, 1, 0 }, before!);
                Assert.Equal(new uint[] { 2, 0, 1 }, after!);
                observed = after;
                return preview.Constants.Break;
            }));
            preview.Update([2], [2, 1, 1], null);
            Assert.Equal(new uint[] { 2, 0, 1 }, observed!);
            Assert.Throws<ArgumentException>(() => preview.Delete(null, null));
            Assert.Throws<ArgumentException>(() => preview.Delete([], null));
            Assert.Throws<KeyNotFoundException>(() => preview.Delete([99], null));
            Assert.Equal(preview.Constants.Break, preview.Delete([3], (before, after) =>
            {
                Assert.Equal(new uint[] { 3, 0, 3 }, before!);
                Assert.Null(after);
                return preview.Constants.Break;
            }));
            preview.RemoveName(1);
            Assert.Null(preview.GetName(1));
            Assert.Equal(0u, preview.GetByName("original"));
            Assert.Throws<KeyNotFoundException>(() => preview.SetName(99, "missing"));
            Assert.Throws<ArgumentNullException>(() => preview.SetName(1, null!));
            Assert.Throws<KeyNotFoundException>(() => preview.RemoveName(99));
        });
        Assert.Equal("original", tx.GetName(1));
        Assert.Equal(1u, tx.GetLink(2)!.Value.Source);
        Assert.NotNull(tx.GetLink(3));
        Assert.Equal(4u, tx.CreateLink(0, 0));
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void ExhaustedPreviewFailsBeforeConsumingTheRemainingRawAddress()
    {
        var engine = new LinksEngine<byte>(new MemoryCommitLog());
        using var tx = engine.Begin();
        tx.EnsureCreated(byte.MaxValue - 1);
        Assert.Throws<OverflowException>(() => tx.ReserveQueryAllocations(preview => preview.Create(null, null)));
        Assert.Equal(byte.MaxValue, tx.CreateLink(0, 0));
        Assert.Throws<OverflowException>(() => tx.CreateLink(0, 0));
        Assert.Empty(engine.Snapshot().Rows);
    }
}
