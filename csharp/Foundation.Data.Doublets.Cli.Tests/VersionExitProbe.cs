using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

internal static class VersionExitProbe
{
    internal const string Topic = "topic|:parent=雪\0";

    private sealed class CreationProgressLinks(INamedTypesLinks<uint> store, Action<string> phase) : LinksDecoratorBase<uint>(store)
    {
        public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            uint result = base.Create(substitution, handler);
            phase("row-allocated");
            return result;
        }
    }

    internal static uint CreateInitialRow(INamedTypesLinks<uint> store, Action<string> phase)
    {
        var result = new CreationProgressLinks(store, phase).CreateAndUpdate(11u, 12u);
        phase("row-created");
        return result;
    }

    internal static int Run(string directory, string mode, bool automatic, bool names = false)
    {
        FiniteExitChild.Phase("start");
        using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
        FiniteExitChild.Phase("data-open");
        using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
        FiniteExitChild.Phase("log-open");
        using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
        FiniteExitChild.Phase("branches-open");
        var armed = false;
        void FlushData()
        {
            if (armed) throw new IOException("Injected post-decision checkpoint error.");
            data.Flush();
        }
        using var tx = new TransactionsDecorator(data, log, flushData: FlushData, flushLog: log.Flush, trace: true);
        FiniteExitChild.Phase("transactions-open");
        using var vc = new VersionControlDecorator(tx, branches, trace: true);
        FiniteExitChild.Phase("version-open");
        var id = CreateInitialRow(vc, FiniteExitChild.Phase);
        vc.SetName(id, "original");
        FiniteExitChild.Phase("name-written");
        var original = vc.CurrentSequence;
        vc.Branch(Topic);
        FiniteExitChild.Phase("branch-created");
        vc.SwitchBranch(Topic);
        FiniteExitChild.Phase("branch-switched");
        data.Flush(); log.Flush(); branches.Flush();
        FiniteExitChild.Phase("setup-flushed");
        var transaction = automatic ? null : vc.BeginTransaction();
        armed = mode == "checkpoint";
        FiniteExitChild.Phase("cut-armed");
        var error = Record.Exception(() =>
        {
            if (names) vc.SetName(id, "changed 雪|\0\n");
            else vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
            if (!automatic && mode != "pending")
            {
                if (mode == "rollback") transaction!.Rollback();
                else transaction!.Commit();
            }
        });
        FiniteExitChild.Phase("operation-complete");
        if (mode == "checkpoint") Assert.IsType<IOException>(error);
        else Assert.Null(error);
        if (mode is "pending" or "rollback") { data.Flush(); log.Flush(); }
        File.WriteAllText(Path.Combine(directory, "manifest"), $"{id},{original},{tx.LastLoggedSequence}");
        // The test deliberately skips all transaction and mapping disposal.
        FiniteExitChild.Phase("manifest-written");
        Environment.Exit(0);
        return 0;
    }
}
