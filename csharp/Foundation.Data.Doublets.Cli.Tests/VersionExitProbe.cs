using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

internal static class VersionExitProbe
{
    internal const string Topic = "topic|:parent=雪\0";

    internal static int Run(string directory, string mode, bool automatic, bool names = false)
    {
        using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
        using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
        using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
        var armed = false;
        void FlushData()
        {
            if (armed) throw new IOException("Injected post-decision checkpoint error.");
            data.Flush();
        }
        using var tx = new TransactionsDecorator(data, log, flushData: FlushData, flushLog: log.Flush);
        using var vc = new VersionControlDecorator(tx, branches);
        var id = vc.CreateAndUpdate(11u, 12u);
        vc.SetName(id, "original");
        var original = vc.CurrentSequence;
        vc.Branch(Topic);
        vc.SwitchBranch(Topic);
        data.Flush(); log.Flush(); branches.Flush();
        var transaction = automatic ? null : vc.BeginTransaction();
        armed = mode == "checkpoint";
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
        if (mode == "checkpoint") Assert.IsType<IOException>(error);
        else Assert.Null(error);
        if (mode is "pending" or "rollback") { data.Flush(); log.Flush(); }
        File.WriteAllText(Path.Combine(directory, "manifest"), $"{id},{original},{tx.LastLoggedSequence}");
        // The test deliberately skips all transaction and mapping disposal.
        Environment.Exit(0);
        return 0;
    }
}
