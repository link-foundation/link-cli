using Foundation.Data.Doublets.Cli.Engine;
using Xunit;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class AuthorFenceTests
{
    [Fact]
    public void ConstructorAndBindingValidateAuthorityAndSelectedCapabilities()
    {
        var s = new Store { Durable = false };
        foreach (var options in new AuthorFenceOptions[] { new(MaxEntries: 4097), new(MaxBytes: 1_048_577), new(RequireDurableFlush: true) })
            Assert.Throws<InvalidDataException>(() => new AuthorFence(s, "fence", A, options));
        Assert.Throws<InvalidDataException>(() => new AuthorFence(null!, "fence", A));
        Assert.Throws<InvalidDataException>(() => new AuthorFence(s, null!, A));
        Assert.Throws<InvalidDataException>(() => new AuthorFence(s, "fence", null!));
        var f = new AuthorFence(s, "fence", A);
        foreach (var claim in new AuthorClaim?[] { null, new(T2, T1, 1), new(A, "g".PadLeft(32, '0'), 1) })
            Assert.Throws<InvalidDataException>(() => f.Bind(claim!));
        var guard = f.Bind(new(A, T1, 1)); Assert.Same(s, guard.Store);
        Assert.False(guard.SupportsDurableFlush); s.Durable = true; Assert.True(guard.SupportsDurableFlush);
        Assert.Throws<InvalidDataException>(() => new AuthorFence(s, "fence", A, new(MaxBytes: 1)).Claim(0, T1));
        Assert.Empty(s.Values); Assert.Equal(0, s.Calls);
    }

    [Fact]
    public void MalformedProviderFramesCannotReachPublication()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A);
        IReadOnlyList<JournalVersion>?[] frames = [null, [], [new(0, []), new(0, [])], [null!], [new(0, null!)], [new(1, [null!])]];
        frames = frames.Append([new(1, [$"[\"author-fence1\",\"{A}\"]", $"[\"unknown-author1\",\"{T1}\",\"1\"]"])]).ToArray();
        foreach (var frame in frames)
        {
            s.ReadOverride = _ => frame!;
            Assert.Throws<InvalidDataException>(() => f.Snapshot());
        }
        Assert.Empty(s.Values); Assert.Equal(0, s.Calls);
    }

    [Fact]
    public void ActiveRetriesRequireOriginalPredecessorEvenAtGenerationCeiling()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A); var claim = f.Claim(0, T1);
        Assert.Throws<InvalidOperationException>(() => f.Claim(ulong.MaxValue, T1));
        Assert.Throws<InvalidOperationException>(() => f.Claim(1, T1));
        s.Values["fence"] = s.Values["fence"] with { Generation = ulong.MaxValue }; string before = s.Bytes();
        Assert.Throws<InvalidDataException>(() => f.Claim(1, T2));
        Assert.Equal(claim, f.Claim(0, T1)); Assert.Equal(before, s.Bytes()); Assert.Equal(1, s.Calls);
    }

    [Fact]
    public void CompetingSamePredecessorClaimHasOneWinnerAndNoAutomaticReplay()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A); AuthorClaim? winner = null;
        s.Before = () => winner = f.Claim(0, T2);
        Assert.Throws<InvalidOperationException>(() => f.Claim(0, T1));
        Assert.Equal(T2, winner!.Token); Assert.Equal(winner, f.Claim(0, T2)); Assert.Equal(2, s.Calls);
        Assert.Single(f.Snapshot().Claims);
    }

    [Fact]
    public void AbsentOwnershipAndFiniteByteAdmissionPreserveTheProvider()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A, new(MaxBytes: 160)); var absent = f.Bind(new(A, T1, 1));
        Assert.Throws<InvalidOperationException>(() => absent.Read(["data"]));
        Assert.Throws<InvalidOperationException>(() => absent.CompareExchange(["data"], [new(0, [])], [new[] { "bad" }]));
        Assert.Empty(s.Values); Assert.Equal(0, s.Calls);
        var first = f.Claim(0, T1); string before = s.Bytes();
        Assert.Throws<InvalidDataException>(() => f.Claim(1, T2));
        Assert.Equal(first, f.Claim(0, T1)); Assert.Equal(before, s.Bytes()); Assert.Equal(1, s.Calls);
    }

    [Fact]
    public void GuardOwnsPendingArgumentsAndRejectsMalformedGroups()
    {
        var s = new Store(); var f = new AuthorFence(s, "fence", A); var guard = f.Bind(f.Claim(0, T1));
        string before = s.Bytes();
        foreach (var keys in new string[]?[] { null, [], ["bad/key"], ["data\n"] })
            Assert.Throws<InvalidDataException>(() => guard.Read(keys!));
        Assert.Throws<InvalidDataException>(() => guard.CompareExchange(["data"], null!, [null]));
        Assert.Throws<InvalidDataException>(() => guard.CompareExchange(["data"], [new(0, [])], null!));
        Assert.Throws<InvalidDataException>(() => guard.CompareExchange(["data"], [new(0, [])], []));
        Assert.Equal(before, s.Bytes()); Assert.Equal(1, s.Calls);

        string[] selected = ["data"]; JournalVersion[] expected = [new(0, [])]; string[][] updates = [["chosen"]];
        s.BeforeRead = () => { selected[0] = "fence"; expected[0] = new(99, []); updates[0][0] = "altered"; };
        Assert.True(guard.CompareExchange(selected, expected, updates));
        Assert.Equal("fence", selected[0]); Assert.Equal(new[] { "chosen" }, s.Values["data"].Entries);
        Assert.Equal(1UL, f.Snapshot().Epoch);
    }
}
