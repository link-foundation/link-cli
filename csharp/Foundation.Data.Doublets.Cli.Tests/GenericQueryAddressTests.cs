using System.Numerics;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed class GenericQueryAddressTests
{
    private static void WidePoint<T>(T id)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        var engine = new LinksEngine<T>(new MemoryCommitLog());
        using var tx = engine.Begin();
        tx.EnsureCreated(id);
        tx.UpdateLink(id, id, id);
        var changes = new List<(Link<T>, Link<T>)>();
        AdvancedMixedQueryProcessor<T>.ProcessQuery(tx, new()
        {
            Query = $"(({id}: {id} {id})) (({id}: {id} {id}))",
            ChangesHandler = (before, after) =>
            {
                Assert.NotNull(before);
                Assert.NotNull(after);
                changes.Add((new(before[0], before[1], before[2]), new(after[0], after[1], after[2])));
                return tx.Constants.Continue;
            },
        });
        Assert.Equal(new[] { (new Link<T>(id, id, id), new Link<T>(id, id, id)) }, changes);
        Assert.Equal(new Link<T>(id, id, id), tx.GetLink(id));
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void SelectedUnsignedAddressesReachTheirOwnNumericRange()
    {
        WidePoint((ushort)(ushort.MaxValue - 1));
        WidePoint(uint.MaxValue - 1);
        WidePoint(ulong.MaxValue - 1);
        WidePoint(UInt128.MaxValue - 1);
        // A wider selected address keeps the complete stored u64 range below Any.
        WidePoint<UInt128>(ulong.MaxValue);
    }

    [Fact]
    public void QuotedNumericNameAndFullWidthAddressRemainDistinct()
    {
        var engine = new LinksEngine<UInt128>(new MemoryCommitLog());
        using var tx = engine.Begin();
        UInt128 id = ulong.MaxValue;
        tx.EnsureCreated(1);
        tx.UpdateLink(1, 1, 1);
        tx.SetName(1, id.ToString());
        tx.EnsureCreated(id);
        tx.UpdateLink(id, id, id);
        AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(tx,
            $"(('{id}': '{id}' '{id}')) (('{id}': '{id}' '{id}'))");
        AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(tx,
            $"(({id}: {id} {id})) (({id}: {id} {id}))");
        Assert.Equal(new Link<UInt128>(1, 1, 1), tx.GetLink(1));
        Assert.Equal(new Link<UInt128>(id, id, id), tx.GetLink(id));
        Assert.Equal(id.ToString(), tx.GetName(1));
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void ExistingUintOptionsAndPatternFacadeRemainUsable()
    {
        AdvancedMixedQueryProcessor.Options options = "(() ())";
        options.Trace = false;
        options.AutoCreateMissingReferences = true;
        options.ChangesHandler = (_, _) => 1;
        var leaf = new AdvancedMixedQueryProcessor.Pattern("$x");
        var pattern = new AdvancedMixedQueryProcessor.Pattern("$i", leaf, leaf);
        AdvancedMixedQueryProcessor.Pattern? source = pattern.Source;
        Assert.Same(leaf, source);
        Assert.True(leaf.IsLeaf);
        Assert.False(pattern.IsLeaf);
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        AdvancedMixedQueryProcessor.ProcessQuery(tx, options);
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void GenericMissingOwnerAndOptionsPreserveArgumentDiagnostics()
    {
        var owner = Assert.Throws<ArgumentNullException>(() =>
            AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(null!, new()));
        Assert.Equal("links", owner.ParamName);
        var engine = new LinksEngine<UInt128>(new MemoryCommitLog());
        using var tx = engine.Begin();
        var options = Assert.Throws<ArgumentNullException>(() =>
            AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(tx, null!));
        Assert.Equal("options", options.ParamName);
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public void ZeroReferencesRemainBoundDuringSwapsEqualityAndJoins(int scenario)
    {
        var (seed, query, expected) = scenario switch
        {
            0 => (new[] { new Link<uint>(1, 1, 1), new Link<uint>(2, 1, 0) },
                "((2: $s $t)) ((2: $t $s))", new[] { new Link<uint>(1, 1, 1), new Link<uint>(2, 0, 1) }),
            1 => (new[] { new Link<uint>(1, 0, 1) },
                "(($i: $x $x)) ()", new[] { new Link<uint>(1, 0, 1) }),
            _ => (new[] { new Link<uint>(1, 0, 1), new Link<uint>(2, 1, 1) },
                "((1: $x *) (2: $x *)) ((1: $x 1))", new[] { new Link<uint>(1, 0, 1), new Link<uint>(2, 1, 1) }),
        };
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        foreach (var row in seed)
        {
            tx.EnsureCreated(row.Index);
            tx.UpdateLink(row.Index, row.Source, row.Target);
        }
        AdvancedMixedQueryProcessor.ProcessQuery(tx, query);
        var rows = new List<Link<uint>>();
        tx.Each(null, raw => { rows.Add(new(raw)); return tx.Constants.Continue; });
        Assert.Equal(expected, rows);
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void StandardWideCallbacksRetainOwnedReadOnlyPartsAfterFollowingEdits()
    {
        var engine = new LinksEngine<UInt128>(new MemoryCommitLog());
        using var tx = engine.Begin();
        UInt128 high = ulong.MaxValue;
        var observed = new List<IList<UInt128>>();
        tx.Create(null, (before, after) =>
        {
            Assert.Null(before);
            Assert.NotNull(after);
            observed.Add(after);
            return tx.Constants.Continue;
        });
        tx.Update([1], [1, high, 0], (before, after) =>
        {
            Assert.NotNull(before);
            Assert.NotNull(after);
            observed.Add(before);
            observed.Add(after);
            return tx.Constants.Continue;
        });
        tx.Each(null, row =>
        {
            Assert.NotNull(row);
            observed.Add(row);
            return tx.Constants.Continue;
        });
        tx.Delete([1], (before, after) =>
        {
            Assert.NotNull(before);
            Assert.Null(after);
            observed.Add(before);
            return tx.Constants.Continue;
        });
        Assert.Equal(5, observed.Count);
        Assert.Equal(new UInt128[] { 1, 0, 0 }, observed[0]);
        Assert.Equal(new UInt128[] { 1, 0, 0 }, observed[1]);
        foreach (var row in observed.Skip(2))
        {
            Assert.Equal(new UInt128[] { 1, high, 0 }, row);
            Assert.Equal(3, row.Count);
            Assert.True(row.IsReadOnly);
            Assert.Equal(0, row.IndexOf(1));
            Assert.Equal(1, row.IndexOf(high));
            Assert.Equal(2, row.IndexOf(0));
            Assert.Equal(-1, row.IndexOf(2));
            Assert.True(row.Contains(high));
            Assert.False(row.Contains(2));
            var copy = new UInt128[5];
            row.CopyTo(copy, 1);
            Assert.Equal(new UInt128[] { 0, 1, high, 0, 0 }, copy);
            copy[2] = 0;
            Assert.Equal(high, row[1]);
            Assert.Throws<ArgumentOutOfRangeException>(() => row[-1]);
            Assert.Throws<ArgumentOutOfRangeException>(() => row[3]);
            Assert.Throws<NotSupportedException>(() => row[0] = 2);
            Assert.Throws<NotSupportedException>(() => row.Add(2));
            Assert.Throws<NotSupportedException>(() => row.Insert(0, 2));
            Assert.Throws<NotSupportedException>(() => row.Remove(1));
            Assert.Throws<NotSupportedException>(() => row.RemoveAt(0));
            Assert.Throws<NotSupportedException>(row.Clear);
        }
        Assert.Equal(UInt128.Zero, tx.Count(null));
        Assert.Empty(engine.Snapshot().Rows);
    }
}
