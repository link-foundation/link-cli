using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
  public class VersionControlDecoratorTests
  {
    [Fact]
    public void DefaultBranchExistsOnFirstOpen()
    {
      RunWithVc((vc, _, _) =>
      {
        Assert.Equal(VersionControlDecorator.DefaultBranchName, vc.CurrentBranch);
        var branches = vc.ListBranches();
        Assert.Single(branches);
        Assert.Equal(VersionControlDecorator.DefaultBranchName, branches[0].Name);
      });
    }

    [Fact]
    public void NewTransitionsAreAttributedToCurrentBranch()
    {
      RunWithVc((vc, tx, _) =>
      {
        var a = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        var head = tx.LastLoggedSequence;
        Assert.True(head >= 2, $"CreateAndUpdate must produce at least two transitions (got {head}).");
        Assert.Equal(head, vc.CurrentSequence);
      });
    }

    [Fact]
    public void CheckoutToZeroRewindsEverything()
    {
      RunWithVc((vc, tx, _) =>
      {
        var a = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        var b = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        Assert.True(vc.Exists(a));
        Assert.True(vc.Exists(b));

        vc.Checkout(0);

        Assert.False(vc.Exists(a), "All links must be rewound after checkout 0.");
        Assert.False(vc.Exists(b));
        Assert.Equal(0, vc.CurrentSequence);
      });
    }

    [Fact]
    public void CheckoutAndForwardReplayRestoresState()
    {
      RunWithVc((vc, tx, _) =>
      {
        var a = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        var afterFirst = tx.LastLoggedSequence;
        var b = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        var afterSecond = tx.LastLoggedSequence;

        vc.Checkout(afterFirst);
        Assert.True(vc.Exists(a), "First link must remain after partial rewind.");
        Assert.False(vc.Exists(b), "Second link must disappear after partial rewind.");

        vc.Checkout(afterSecond);
        Assert.True(vc.Exists(a));
        Assert.True(vc.Exists(b), "Second link must reappear after forward checkout.");
      });
    }

    [Fact]
    public void BranchForksFromCurrentHead()
    {
      RunWithVc((vc, tx, _) =>
      {
        vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        var headBeforeBranch = vc.CurrentSequence;

        vc.Branch("feature");
        Assert.Contains(vc.ListBranches(), b => b.Name == "feature");
      });
    }

    [Fact]
    public void SwitchBranchAppliesAndRewindsTransitions()
    {
      RunWithVc((vc, tx, _) =>
      {
        var a = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        var headBeforeBranch = vc.CurrentSequence;

        vc.Branch("feature");
        vc.SwitchBranch("feature");
        Assert.Equal("feature", vc.CurrentBranch);

        var b = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        Assert.True(vc.Exists(b));
        var featureHead = vc.CurrentSequence;

        vc.SwitchBranch(VersionControlDecorator.DefaultBranchName);
        Assert.Equal(VersionControlDecorator.DefaultBranchName, vc.CurrentBranch);
        Assert.True(vc.Exists(a), "Main-branch link must remain after switching back.");
        Assert.False(vc.Exists(b), "Feature-branch link must disappear after switching back to main.");
        Assert.Equal(headBeforeBranch, vc.CurrentSequence);

        vc.SwitchBranch("feature");
        Assert.True(vc.Exists(a));
        Assert.True(vc.Exists(b), "Feature-branch link must reappear after switching back to feature.");
        Assert.Equal(featureHead, vc.CurrentSequence);
      });
    }

    [Fact]
    public void TagPointsToCurrentHead()
    {
      RunWithVc((vc, tx, _) =>
      {
        vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        vc.Tag("v1");
        Assert.True(vc.TryGetTag("v1", out var seq));
        Assert.Equal(vc.CurrentSequence, seq);
        Assert.Contains("v1", vc.ListTags().Keys);
      });
    }

    [Fact]
    public void BranchFromExplicitSeqUsesGivenPoint()
    {
      RunWithVc((vc, tx, _) =>
      {
        vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        var firstHead = vc.CurrentSequence;
        vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);

        vc.Branch("backport", from: firstHead);
        var branchInfo = vc.ListBranches().Single(b => b.Name == "backport");
        Assert.Equal(firstHead, branchInfo.ForkSeq);
      });
    }

    [Fact]
    public void RecoverRebuildsStateFromBranchesStore()
    {
      // Recovery is exercised here by attaching a *second* VC decorator
      // to the same live branches store, which is equivalent in behaviour
      // to reopening the underlying file (the file-mapped store is shared).
      RunWithVc((vc, _, _) =>
      {
        vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        vc.Tag("checkpoint");
        vc.Branch("feature");

        // Force a fresh decorator over the same in-process VC store.
        var branchesStore = GetBranchesStore(vc);
        var transactions = GetTransactions(vc);
        var reopened = new VersionControlDecorator(transactions, branchesStore);
        Assert.Contains(reopened.ListBranches(), b => b.Name == "feature");
        Assert.True(reopened.TryGetTag("checkpoint", out _));
      });
    }

    private static INamedTypesLinks<uint> GetBranchesStore(VersionControlDecorator vc)
    {
      return (INamedTypesLinks<uint>)typeof(VersionControlDecorator)
        .GetField("_branchesStore", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)!
        .GetValue(vc)!;
    }

    private static TransactionsDecorator GetTransactions(VersionControlDecorator vc)
    {
      return (TransactionsDecorator)typeof(VersionControlDecorator)
        .GetField("_transactions", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)!
        .GetValue(vc)!;
    }

    [Fact]
    public void CheckoutOutOfRangeThrows()
    {
      RunWithVc((vc, tx, _) =>
      {
        vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
        Assert.Throws<InvalidOperationException>(() => vc.Checkout(999));
      });
    }

    [Fact]
    public void DuplicateBranchThrows()
    {
      RunWithVc((vc, _, _) =>
      {
        vc.Branch("feature");
        Assert.Throws<InvalidOperationException>(() => vc.Branch("feature"));
      });
    }

    private static void RunWithVc(Action<VersionControlDecorator, TransactionsDecorator, NamedTypesDecorator<uint>> action)
    {
      var dataFile = Path.GetTempFileName();
      var logFile = Path.GetTempFileName();
      var vcFile = Path.GetTempFileName();
      NamedTypesDecorator<uint>? dataLinks = null;
      NamedTypesDecorator<uint>? logLinks = null;
      NamedTypesDecorator<uint>? vcLinks = null;
      TransactionsDecorator? tx = null;
      try
      {
        dataLinks = new NamedTypesDecorator<uint>(dataFile);
        logLinks = new NamedTypesDecorator<uint>(logFile);
        vcLinks = new NamedTypesDecorator<uint>(vcFile);
        tx = new TransactionsDecorator(dataLinks, logLinks);
        var vc = new VersionControlDecorator(tx, vcLinks);
        action(vc, tx, dataLinks);
      }
      finally
      {
        tx?.Shutdown();
        Cleanup(dataFile);
        Cleanup(logFile);
        Cleanup(vcFile);
        if (dataLinks is not null) Cleanup(dataLinks.NamedLinksDatabaseFileName);
        if (logLinks is not null) Cleanup(logLinks.NamedLinksDatabaseFileName);
        if (vcLinks is not null) Cleanup(vcLinks.NamedLinksDatabaseFileName);
      }
    }

    private static void Cleanup(string path)
    {
      if (File.Exists(path)) File.Delete(path);
    }
  }
}
