using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionPublicationIntentFailureTests
{
    internal sealed class Fault
    {
        internal string? Point;
        internal int Flushes;
        internal Action<string>? OnCheck;
        internal void Check(string point)
        {
            OnCheck?.Invoke(point);
            if (Point != point) return;
            Point = null;
            throw new IOException("injected " + point);
        }
    }

    internal sealed class Metadata(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        private uint _intent;
        public override string? GetName(uint link)
        {
            var name = base.GetName(link);
            if ((name?.StartsWith("__vc:pending:", StringComparison.Ordinal) == true || name?.StartsWith("__vc:move:", StringComparison.Ordinal) == true)) _intent = link;
            return name;
        }
        public override uint SetName(uint link, string name)
        {
            var phase = (name.StartsWith("__vc:pending:", StringComparison.Ordinal) || name.StartsWith("__vc:move:", StringComparison.Ordinal)) ? "intent-write"
                : name.StartsWith("__vc:trans:", StringComparison.Ordinal) ? "transition"
                : name.StartsWith("__vc:branch:", StringComparison.Ordinal) ? "head"
                : name.StartsWith("__vc:current=", StringComparison.Ordinal) ? "current"
                : name.StartsWith("__vc:applied=", StringComparison.Ordinal) ? "applied" : null;
            if (phase == "intent-write") _intent = link;
            if (phase is not null) fault.Check(phase + "-before");
            var result = base.SetName(link, name);
            if (phase is not null) fault.Check(phase + "-after");
            return result;
        }
        public override void Flush()
        {
            var phase = ++fault.Flushes switch { 1 => "intent-flush", 2 => "publication-flush", _ => "cleanup-flush" };
            fault.Check("flush-before"); fault.Check(phase + "-before");
            base.Flush();
            fault.Check(phase + "-after"); fault.Check("flush-after");
        }
        public override void RemoveName(uint link)
        {
            var name = GetName(link);
            var intent = name?.StartsWith("__vc:pending:", StringComparison.Ordinal) == true || name?.StartsWith("__vc:move:", StringComparison.Ordinal) == true;
            if (intent) fault.Check("intent-remove-before");
            base.RemoveName(link);
            if (intent) fault.Check("intent-remove-after");
        }
        public override uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler)
        {
            var intent = restriction is [var link] && link == _intent;
            if (intent) fault.Check("intent-delete-before");
            var result = base.Delete(restriction, handler);
            if (intent) fault.Check("intent-delete-after");
            return result;
        }
    }

    public static TheoryData<string, bool, string> Cases
    {
        get
        {
            var data = new TheoryData<string, bool, string>();
            foreach (var phase in new[] { "intent-write", "intent-flush", "transition", "head", "applied", "publication-flush", "intent-remove", "intent-delete", "cleanup-flush" })
                foreach (var edge in new[] { "before", "after" })
                {
                    foreach (var entry in new[] { "commit", "rollback", "dispose" }) data.Add(phase + "-" + edge, false, entry);
                    foreach (var entry in new[] { "owner", "graph" }) data.Add(phase + "-" + edge, true, entry);
                }
            return data;
        }
    }

    [Theory(Timeout = 30000)]
    [MemberData(nameof(Cases))]
    public async Task EveryPublicationPhaseRetainsCleanupAndAdmission(string point, bool automatic, string entry)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("version-intent-fault-").FullName;
        try
        {
            uint id;
            long original, attempted, following;
            var committed = !point.StartsWith("intent-write", StringComparison.Ordinal)
                && !point.StartsWith("intent-flush", StringComparison.Ordinal) || entry == "commit";
            using (var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links")))
            {
                var fault = new Fault();
                using var branches = new Metadata(Path.Combine(directory, "branches.links"), fault);
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches, trace: true);
                Assert.True(vc.SupportsDurablePublication);
                id = vc.CreateAndUpdate(11u, 12u);
                original = vc.CurrentSequence;
                vc.Branch(VersionExitProbe.Topic); vc.SwitchBranch(VersionExitProbe.Topic);
                var pending = automatic ? null : vc.BeginTransaction();
                if (!automatic) vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                fault.Flushes = 0; fault.Point = point;
                var error = Assert.Throws<IOException>(() =>
                {
                    if (automatic) vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                    else pending!.Commit();
                });
                Assert.Contains(point, error.Message);
                Assert.Null(fault.Point);
                attempted = tx.LastLoggedSequence;
                Assert.Equal(21u, vc.GetSource(id));
                void Fences()
                {
                    Assert.Throws<InvalidOperationException>(() => vc.CreateAndUpdate(51u, 52u));
                    Assert.Throws<InvalidOperationException>(() => vc.Update(new[] { id }, new[] { id, 51u, 52u }, null));
                    Assert.Throws<InvalidOperationException>(() => vc.Delete(new[] { id }, null));
                    Assert.Throws<InvalidOperationException>(() => vc.SetName(id, "blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.RemoveName(id));
                    Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
                    Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.SwitchBranch("main"));
                    Assert.Throws<InvalidOperationException>(() => vc.Tag("blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.Checkout(0));
                    Assert.Throws<InvalidOperationException>(vc.Recover);
                    Assert.Equal(attempted, tx.LastLoggedSequence);
                }
                void Cleanup()
                {
                    if (entry == "commit") pending!.Commit();
                    else if (entry == "rollback") pending!.Rollback();
                    else if (entry == "dispose") pending!.Dispose();
                    else if (entry == "owner") vc.Dispose();
                    else LinksFacadeDisposer.Dispose(vc);
                }
                Fences();
                fault.Point = "flush-before";
                Assert.Contains("flush-before", Assert.Throws<IOException>(Cleanup).Message);
                Fences();
                Cleanup();
                Assert.Null(fault.Point);
                Assert.Equal(committed ? attempted : original, vc.CurrentSequence);
                if (entry != "graph")
                {
                    Assert.Equal(committed ? 21u : 11u, vc.GetSource(id));
                    vc.Checkout(original);
                    Assert.Equal(11u, vc.GetSource(id));
                    if (committed) vc.Checkout(attempted);
                    else Assert.Throws<InvalidOperationException>(() => vc.Checkout(attempted));
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    following = vc.CurrentSequence;
                }
                else following = 0;
            }
            for (var reopen = 0; reopen < 2; ++reopen)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal(VersionExitProbe.Topic, vc.CurrentBranch);
                if (following == 0)
                {
                    Assert.Equal(committed ? attempted : original, vc.CurrentSequence);
                    Assert.Equal(committed ? 21u : 11u, vc.GetSource(id));
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    following = vc.CurrentSequence;
                }
                Assert.Equal(following, vc.CurrentSequence);
                Assert.Equal(following, vc.ListBranches().Single(b => b.Name == VersionExitProbe.Topic).Head);
                Assert.Equal(31u, vc.GetSource(id));
                var any = branches.Constants.Any;
                Assert.DoesNotContain(branches.All(new Link<uint>(any, any, any)), row =>
                    row is not null && branches.GetName(row[0])?.StartsWith("__vc:pending:", StringComparison.Ordinal) == true);
                data.Flush(); log.Flush(); branches.Flush();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
