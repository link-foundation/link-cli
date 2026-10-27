using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

public class FinitePhaseWatchTests
{
    [Fact]
    public void OnlyCompleteConsecutiveBoundedPhasesExtendIdleAdmission()
    {
        using var watch = new FinitePhaseWatch(TimeSpan.FromSeconds(1));
        watch.Observe("ordinary diagnostic\n@link-progress:1 phase=ready\n@link-prog");
        Assert.Equal(1, watch.LastPhase); Assert.Null(watch.Failure);
        watch.Observe("ress:2 phase=next\r\n"); Assert.Equal(2, watch.LastPhase);
        for (var phase = 3; phase <= 64; phase++) watch.Observe($"@link-progress:{phase} phase=next\n");
        Assert.Equal(64, watch.LastPhase); Assert.False(watch.Token.IsCancellationRequested);
        watch.Observe("@link-progress:65 phase=overflow\n");
        Assert.NotNull(watch.Failure); Assert.True(watch.Token.IsCancellationRequested);
    }
    [Theory]
    [InlineData("@link-progress:1 phase=repeat\n")]
    [InlineData("@link-progress:0 phase=regress\n")]
    [InlineData("@link-progress:3 phase=skip\n")]
    [InlineData("@link-progress:02 phase=noncanonical\n")]
    [InlineData("@link-progress:bad phase=malformed\n")]
    public void InvalidProgressCannotKeepAChildAlive(string invalid)
    {
        using var watch = new FinitePhaseWatch(TimeSpan.FromSeconds(1));
        watch.Observe("@link-progress:1 phase=first\n"); watch.Observe(invalid);
        Assert.NotNull(watch.Failure); Assert.True(watch.Token.IsCancellationRequested);
        Assert.Equal(1, watch.LastPhase);
    }
    [Fact]
    public void OversizedAndPartialProgressIsRejectedWhileOrdinaryDiagnosticsStayBounded()
    {
        using (var watch = new FinitePhaseWatch(TimeSpan.FromSeconds(1)))
        {
            watch.Observe(new string('x', 2048) + "\n@link-progress:1 phase=ready\n");
            Assert.Equal(1, watch.LastPhase); Assert.Null(watch.Failure);
            watch.Observe("@link-progress:2 " + new string('x', 1024) + "\n");
            Assert.NotNull(watch.Failure);
        }
        using var partial = new FinitePhaseWatch(TimeSpan.FromSeconds(1));
        partial.Observe("@link-progress:1 phase=partial"); Assert.Equal(0, partial.LastPhase);
        partial.Complete(); Assert.NotNull(partial.Failure);
    }
}
