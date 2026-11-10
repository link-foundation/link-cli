using System.Text;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

// Console replacement is process-wide. This collection cannot overlap others.
[CollectionDefinition("Diagnostic console", DisableParallelization = true)]
public class DiagnosticConsoleCollection { }

[Collection("Diagnostic console")]
public partial class DiagnosticOutputTests
{
    private sealed class UnavailableWriter : TextWriter
    {
        public override Encoding Encoding => Encoding.UTF8;
        public override void WriteLine(string? value) => throw new IOException("injected diagnostic failure");
    }

    private static T WithoutConsole<T>(Func<T> action)
    {
        var previous = Console.Out;
        using var unavailable = new UnavailableWriter();
        try { Console.SetOut(unavailable); return action(); }
        finally { Console.SetOut(previous); }
    }
    private static void WithoutConsole(Action action) => WithoutConsole(() => { action(); return true; });

    private static void WithStores(Action<NamedTypesDecorator<uint>, NamedTypesDecorator<uint>, string> action)
    {
        var directory = Path.Combine(Path.GetTempPath(), "diagnostic-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            action(data, log, directory);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData("begin")]
    [InlineData("record")]
    [InlineData("commit")]
    [InlineData("rollback")]
    [InlineData("retention")]
    public void DiagnosticFailurePreservesTransactionOutcomeAndFollowingWrites(string stage)
    {
        WithStores((data, log, _) =>
        {
            using var tx = new TransactionsDecorator(data, log, trace: true);
            using var transaction = stage == "begin" ? WithoutConsole(tx.BeginTransaction) : tx.BeginTransaction();
            var id = stage == "record" ? WithoutConsole(() => tx.CreateAndUpdate(0u, 0u)) : tx.CreateAndUpdate(0u, 0u);
            if (stage == "retention") tx.RetentionPolicy = new LogRetentionPolicy.Sized(1);
            if (stage == "rollback") WithoutConsole(transaction.Rollback);
            else if (stage is "commit" or "retention") WithoutConsole(transaction.Commit);
            else transaction.Commit();
            Assert.Equal(stage != "rollback", data.Exists(id));
            Assert.Equal(stage == "rollback", transaction.IsRolledBack);
            Assert.Equal(stage != "rollback", transaction.IsCommitted);
            if (stage == "retention") Assert.Single(tx.Log);
            if (stage != "rollback") Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
            using var following = tx.BeginTransaction();
            var next = tx.CreateAndUpdate(0u, 0u); following.Commit();
            Assert.True(data.Exists(next));
        });
    }

    [Theory]
    [InlineData("branch")]
    [InlineData("switch")]
    [InlineData("tag")]
    [InlineData("checkout")]
    public void DiagnosticFailurePreservesVersionOperations(string stage)
    {
        WithStores((data, log, directory) =>
        {
            using var tx = new TransactionsDecorator(data, log);
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches"));
            using var vc = new VersionControlDecorator(tx, branches, trace: true);
            var id = vc.CreateAndUpdate(0u, 0u); var head = vc.CurrentSequence;
            if (stage != "branch") vc.Branch("feature");
            WithoutConsole(() =>
            {
                switch (stage)
                {
                    case "branch": vc.Branch("feature"); break;
                    case "switch": vc.SwitchBranch("feature"); break;
                    case "tag": vc.Tag("saved"); break;
                    case "checkout": vc.Checkout(0); break;
                }
            });
            Assert.Contains(vc.ListBranches(), branch => branch.Name == "feature");
            if (stage == "switch") Assert.Equal("feature", vc.CurrentBranch);
            if (stage == "tag") { Assert.True(vc.TryGetTag("saved", out var saved)); Assert.Equal(head, saved); }
            if (stage == "checkout") { Assert.False(vc.Exists(id)); vc.Checkout(head); }
            Assert.True(vc.Exists(id));
            Assert.NotEqual(0u, vc.CreateAndUpdate(0u, 0u));
        });
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void DiagnosticFailurePreservesTriggerRegistrationAndRemoval(bool remove)
    {
        WithStores((data, triggers, _) =>
        {
            var links = new PersistentTransformationDecorator(data, triggers, trace: true);
            const string query = "(((1: 1 1)) ((1: 1 2)))";
            if (remove) links.StoreTrigger(PersistentTransformationKind.Always, query);
            WithoutConsole(() =>
            {
                if (remove) Assert.Equal(1, links.RemoveTriggers(query));
                else Assert.NotEqual(0u, links.StoreTrigger(PersistentTransformationKind.Always, query));
            });
            if (remove) Assert.Empty(links.GetTriggers());
            else Assert.Single(links.GetTriggers());
        });
    }
}
