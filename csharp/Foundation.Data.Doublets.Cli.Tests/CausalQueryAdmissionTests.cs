using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed partial class CausalQueryTests
{
    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    public void OriginalBytesDepthAndNodesAreAdmittedBeforePrivateWrites(int kind)
    {
        using var tx = Session(new Store());
        var query = kind switch
        {
            0 => "#" + new string('x', 1024 * 1024),
            1 => "#" + new string('雪', 350000),
            2 => new string('(', 65) + new string(')', 65),
            _ => "() (" + string.Concat(Enumerable.Repeat("(1: 1 1) ", 1400)) + ")"
        };
        Assert.ThrowsAny<Exception>(() => Query(tx.QueryLinks, query));
        Assert.Empty(tx.Rows());
        Assert.Equal(1UL, tx.CreateLink(0, 0));
        Query(tx.QueryLinks, "# " + new string('x', 1024 * 1024 - 2));
        Query(tx.QueryLinks, new string('(', 64) + new string(')', 64));
        Query(tx.QueryLinks, "() ()");
    }

    [Fact]
    public void ResultLimitRejectsBeforeMaterializingAnUnboundedMatchList()
    {
        using var tx = Session(new Store(), 1, 5000);
        for (var i = 0; i < 4097; i++) tx.CreateLink(0, 0);
        var before = tx.Rows();
        Assert.Throws<InvalidOperationException>(() => Query(tx.QueryLinks, "(($i: $s $t)) ()"));
        Assert.Equal(before, tx.Rows());
        Assert.Equal(4098UL, tx.CreateLink(0, 0));
        Assert.Equal((UInt128)1, tx.QueryLinks.Count([1]));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void IndependentJoinsAndLongVariableBindingsHaveFiniteAdmission(bool longKeys)
    {
        using var tx = Session(new Store(), 1, 2000);
        for (var i = 0; i < 100; i++) tx.CreateLink(0, 0);
        var key = longKeys ? "$" + new string('x', 50000) : "$i";
        var query = longKeys ? $"(({key}: $s $t)) ()" : "(($i: $s $t) ($j: $a $b)) ()";
        var before = tx.Rows();
        var error = Assert.Throws<InvalidOperationException>(() => Query(tx.QueryLinks, query));
        Assert.Contains("binding", error.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(before, tx.Rows());
        Assert.Equal(101UL, tx.CreateLink(0, 0));
    }

    [Fact]
    public void DecodedNameReplyLimitRejectsThePreviewBeforePrivateChangesOrCallbacks()
    {
        var store = new Store();
        using var tx = Session(store, 1, 1000);
        for (var i = 0; i < 64; i++)
        {
            var id = tx.CreateLink(0, 0);
            tx.UpdateLink(id, id, id == 1 ? 1 : id - 1);
            tx.SetName(id, id.ToString("D4") + new string('n', 4092));
        }
        var before = tx.Rows(); var calls = 0;
        var error = Assert.Throws<InvalidOperationException>(() => AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(tx.QueryLinks, new()
        {
            Query = "(($i: $s $t)) (($i: $t $s))",
            ChangesHandler = (_, _) => { calls++; return tx.QueryLinks.Constants.Continue; }
        }));
        Assert.Contains("reply", error.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(0, calls);
        Assert.Equal(before, tx.Rows());
        Assert.Empty(store.Values);
        Assert.Equal(65UL, tx.CreateLink(0, 0));
        Query(tx.QueryLinks, "((1: $s $t)) ((1: $s $t))");
    }

    [Fact]
    public void EscapedPreviewAndCallbacksStayDetachedAndExplicitEnsuresRetireOnlyTheirReservation()
    {
        using var tx = Session(new Store());
        var links = tx.QueryLinks;
        INamedTypesLinks<UInt128>? detached = null;
        links.ReserveQueryAllocations(preview =>
        {
            detached = preview;
            preview.Create(null, null); preview.Create(null, null);
        });
        Assert.Empty(tx.Rows());
        links.TryEnsureCreated(2);
        UInt128 first = 0;
        links.Create(null, (_, row) => { first = row![0]; return links.Constants.Continue; });
        Assert.Equal(UInt128.One, first);
        Assert.Equal(3UL, tx.CreateLink(0, 0));
        tx.Prepare(Token(100));
        detached!.TryEnsureCreated(8);
        Assert.Equal((UInt128)3, detached.Count(null));
        Assert.Equal(new[] { 1UL, 2UL, 3UL }, tx.Rows().Select(row => row.Link.Index));
    }

    [Fact]
    public void CheckedOperationsRejectWideStoredPartsBeforeMutationAndPreserveOwnedCallbacks()
    {
        using var tx = Session(new Store());
        var links = tx.QueryLinks;
        links.TryEnsureCreated(1);
        var any = links.Constants.Any;
        Action[] invalid = [() => links.Update([1], [1, any, 0], null), () => links.Update([1], [1, 0, any], null),
            () => links.Update([0], [0, 0, 0], null), () => links.Update(null, null, null), () => links.Delete(null, null),
            () => links.Delete([any], null), () => links.TryEnsureCreated(0), () => links.TryEnsureCreated(9),
            () => links.Count([1, 2, 3, 4]), () => links.Count([(UInt128)ulong.MaxValue + 1]),
            () => links.ReserveQueryAllocations(null!)];
        foreach (var action in invalid) Assert.ThrowsAny<Exception>(action);
        Assert.Equal(0UL, tx.Rows().Single().Link.Source);
        var observations = new List<IList<UInt128>>();
        links.Update([1], [1, 1, 0], (before, after) =>
        { observations.Add(before!); observations.Add(after!); return links.Constants.Continue; });
        links.SetName(1, "named");
        Assert.Equal(UInt128.One, links.GetByName("named"));
        Assert.Equal("named", links.GetName(1));
        Assert.Equal("named", links.CaptureNames()[1]);
        links.RemoveName(1);
        links.Each(null, row => { observations.Add(row!); return links.Constants.Continue; });
        links.Delete([1], (before, _) => { observations.Add(before!); return links.Constants.Continue; });
        Assert.Equal(new UInt128[] { 1, 0, 0 }, observations[0]);
        foreach (var row in observations.Skip(1)) Assert.Equal(new UInt128[] { 1, 1, 0 }, row);
        Assert.Equal((UInt128)0, links.Count(null));
    }
}
