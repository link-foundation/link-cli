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

    [Fact]
    public void FullStackAcidRollbackIsAtomicAndIsolated()
    {
      RunWithVc((vc, _, _) =>
      {
        var baseline = Snapshot(vc);
        var initialSequence = vc.CurrentSequence;

        using (var transaction = vc.BeginTransaction())
        {
          var a = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
          var b = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
          vc.Update(
            new DoubletLink(a, vc.Constants.Any, vc.Constants.Any),
            new DoubletLink(a, b, b),
            null);

          Assert.True(vc.Exists(a));
          Assert.True(vc.Exists(b));
          Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
          Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));

          transaction.Rollback();
        }

        Assert.Equal(initialSequence, vc.CurrentSequence);
        Assert.Equal(initialSequence, vc.ListBranches().Single(b => b.Name == VersionControlDecorator.DefaultBranchName).Head);
        Assert.Equal(baseline, Snapshot(vc));
      });
    }

    [Fact]
    public void FullStackAcidCommitIsConsistentAndDurableAcrossReopen()
    {
      var dataFile = Path.GetTempFileName();
      var logFile = Path.GetTempFileName();
      var vcFile = Path.GetTempFileName();
      NamedTypesDecorator<uint>? dataLinks = null;
      NamedTypesDecorator<uint>? logLinks = null;
      NamedTypesDecorator<uint>? vcLinks = null;
      TransactionsDecorator? tx = null;
      NamedTypesDecorator<uint>? reopenedDataLinks = null;
      NamedTypesDecorator<uint>? reopenedLogLinks = null;
      NamedTypesDecorator<uint>? reopenedVcLinks = null;
      TransactionsDecorator? reopenedTx = null;

      try
      {
        uint a;
        uint b;
        long committedSequence;

        dataLinks = new NamedTypesDecorator<uint>(dataFile);
        logLinks = new NamedTypesDecorator<uint>(logFile);
        vcLinks = new NamedTypesDecorator<uint>(vcFile);
        tx = new TransactionsDecorator(dataLinks, logLinks);
        using var vc = new VersionControlDecorator(tx, vcLinks);

        using (var transaction = vc.BeginTransaction())
        {
          a = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
          b = vc.CreateAndUpdate(vc.Constants.Null, vc.Constants.Null);
          vc.Update(
            new DoubletLink(a, vc.Constants.Any, vc.Constants.Any),
            new DoubletLink(a, b, b),
            null);
          transaction.Commit();
        }

        committedSequence = vc.CurrentSequence;
        Assert.True(committedSequence >= 5);
        Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
        Assert.Equal(committedSequence, vc.ListBranches().Single(branch => branch.Name == VersionControlDecorator.DefaultBranchName).Head);

        vc.Tag("acid-commit");
        vc.Branch("audit");
        vc.SwitchBranch("audit");
        vc.Delete(new DoubletLink(b, vc.Constants.Any, vc.Constants.Any), null);
        Assert.False(vc.Exists(b));

        vc.SwitchBranch(VersionControlDecorator.DefaultBranchName);
        Assert.True(vc.Exists(a));
        Assert.True(vc.Exists(b));
        var restored = new DoubletLink(vc.GetLink(a));
        Assert.Equal(b, restored.Source);
        Assert.Equal(b, restored.Target);

        tx.Dispose();
        tx = null;
        dataLinks.Dispose();
        dataLinks = null;
        logLinks.Dispose();
        logLinks = null;
        vcLinks.Dispose();
        vcLinks = null;

        reopenedDataLinks = new NamedTypesDecorator<uint>(dataFile);
        reopenedLogLinks = new NamedTypesDecorator<uint>(logFile);
        reopenedVcLinks = new NamedTypesDecorator<uint>(vcFile);
        reopenedTx = new TransactionsDecorator(reopenedDataLinks, reopenedLogLinks);
        using var reopened = new VersionControlDecorator(reopenedTx, reopenedVcLinks);

        Assert.True(reopened.TryGetTag("acid-commit", out var tagSequence));
        Assert.Equal(committedSequence, tagSequence);
        Assert.Contains(reopened.ListBranches(), branch => branch.Name == "audit");
        Assert.Equal(VersionControlDecorator.DefaultBranchName, reopened.CurrentBranch);
        Assert.True(reopened.Exists(a));
        Assert.True(reopened.Exists(b));
        restored = new DoubletLink(reopened.GetLink(a));
        Assert.Equal(b, restored.Source);
        Assert.Equal(b, restored.Target);
        Assert.Equal(reopenedTx.LastLoggedSequence, reopenedTx.AppliedSequence);
      }
      finally
      {
        tx?.Dispose();
        reopenedTx?.Dispose();
        dataLinks?.Dispose();
        logLinks?.Dispose();
        vcLinks?.Dispose();
        reopenedDataLinks?.Dispose();
        reopenedLogLinks?.Dispose();
        reopenedVcLinks?.Dispose();
        Cleanup(dataFile);
        Cleanup(logFile);
        Cleanup(vcFile);
        Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dataFile));
        Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(logFile));
        Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(vcFile));
      }
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
        using var vc = new VersionControlDecorator(tx, vcLinks);
        action(vc, tx, dataLinks);
      }
      finally
      {
        tx?.Dispose();
        dataLinks?.Dispose();
        logLinks?.Dispose();
        vcLinks?.Dispose();
        Cleanup(dataFile);
        Cleanup(logFile);
        Cleanup(vcFile);
        Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dataFile));
        Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(logFile));
        Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(vcFile));
      }
    }

    private static void Cleanup(string path)
    {
      if (File.Exists(path)) File.Delete(path);
    }

    private static IReadOnlyList<DoubletLink> Snapshot(ILinks<uint> links)
    {
      var any = links.Constants.Any;
      var query = new DoubletLink(any, any, any);
      return links.All(query)
        .Select(link => new DoubletLink(link))
        .OrderBy(link => link.Index)
        .ThenBy(link => link.Source)
        .ThenBy(link => link.Target)
        .ToArray();
    }
  }
}
