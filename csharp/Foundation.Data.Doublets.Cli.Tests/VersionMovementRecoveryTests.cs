using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMovementRecoveryTests
{
    [Theory(Timeout = 30000)]
    [InlineData("missing-from")]
    [InlineData("missing-to")]
    [InlineData("unreachable-from")]
    [InlineData("unreachable-to")]
    [InlineData("missing-undo")]
    [InlineData("extra-redo")]
    [InlineData("wrong-payload")]
    [InlineData("wrong-decision")]
    [InlineData("wrong-current")]
    [InlineData("wrong-applied")]
    [InlineData("impossible-publication")]
    [InlineData("overlap")]
    [InlineData("duplicate")]
    [InlineData("checksum")]
    [InlineData("duplicate-current")]
    [InlineData("duplicate-applied")]
    [InlineData("duplicate-branch")]
    [InlineData("duplicate-attribution")]
    [InlineData("invalid-branch")]
    [InlineData("invalid-applied")]
    [InlineData("invalid-attribution")]
    [InlineData("cyclic-parent")]
    [InlineData("missing-parent")]
    [InlineData("evicted-record")]
    [InlineData("valid")]
    public async Task RecoveryValidatesBeforeReplayAndRetainsResolvedPayloads(string scenario)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("movement-recovery-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            using var tx = new TransactionsDecorator(data, log);
            using var originalOwner = new VersionControlDecorator(tx, branches);
            var id = originalOwner.CreateAndUpdate(11u, 12u);
            var original = originalOwner.CurrentSequence;
            originalOwner.Branch("original");
            originalOwner.Branch("other");
            originalOwner.Update(new[] { id }, new[] { id, 21u, 22u }, null);
            var attempted = originalOwner.CurrentSequence;
            var record = tx.Log.Last();
            var intent = new VersionControlDecorator.MovementIntent("main", attempted, "original", original, [new JournalRecord<uint>(record)], []);
            uint Marker(string name)
            {
                var link = branches.CreateAndUpdate(0u, 0u);
                branches.SetName(link, name);
                return link;
            }
            uint[] Rows(NamedTypesDecorator<uint> store)
            {
                var any = store.Constants.Any;
                return store.All(new Link<uint>(any, any, any)).Where(r => r is not null).Select(r => r![0]).ToArray();
            }
            void Replace(string prefix, string name)
            {
                var link = Rows(branches).Single(row => branches.GetName(row)?.StartsWith(prefix, StringComparison.Ordinal) == true);
                branches.SetName(link, name);
            }
            intent = scenario switch
            {
                "missing-from" => intent with { FromBranch = "missing" },
                "missing-to" => intent with { ToBranch = "missing" },
                "unreachable-from" => intent with { From = long.MaxValue },
                "unreachable-to" => intent with { To = long.MaxValue },
                "missing-undo" => intent with { Undo = [] },
                "extra-redo" => intent with { Redo = [new JournalRecord<uint>(record)] },
                "wrong-payload" => intent with { Undo = [new JournalRecord<uint>(record with { After = new Link<uint>(id, 91u, 92u) })] },
                "wrong-decision" => intent with { Undo = [new JournalRecord<uint>(record with { TransactionId = Guid.NewGuid() })] },
                _ => intent,
            };
            if (scenario == "wrong-current") Replace("__vc:current=", "__vc:current=other");
            if (scenario == "wrong-applied") Replace("__vc:applied=", "__vc:applied=999");
            if (scenario == "impossible-publication") Replace("__vc:applied=", "__vc:applied=" + original);
            if (scenario == "duplicate-current") Marker("__vc:current=original");
            if (scenario == "duplicate-applied") Marker("__vc:applied=" + original);
            if (scenario == "duplicate-branch") Marker(VersionControlDecorator.EncodeBranchMarker(new("original", "main", 0, original)));
            if (scenario == "duplicate-attribution") Marker("__vc:trans:" + attempted + ":branch=original");
            if (scenario == "invalid-branch") Marker("__vc:branch:broken");
            if (scenario == "invalid-applied") Marker("__vc:applied=broken");
            if (scenario == "invalid-attribution") Marker("__vc:trans:broken");
            if (scenario is "cyclic-parent" or "missing-parent")
            {
                var link = Rows(branches).Single(row => VersionControlDecorator.TryDecodeBranchMarker(branches.GetName(row) ?? "", out var info) && info.Name == "main");
                branches.SetName(link, VersionControlDecorator.EncodeBranchMarker(new("main", scenario == "cyclic-parent" ? "original" : "missing", 0, attempted)));
            }
            if (scenario == "overlap") Marker(VersionControlDecorator.EncodePublicationMarker(new(Guid.NewGuid(), attempted, attempted, "main")));
            if (scenario == "duplicate") Marker(VersionControlDecorator.EncodeMovement(intent with { ToBranch = "main" }));
            var marker = VersionControlDecorator.EncodeMovement(intent);
            if (scenario == "checksum") marker = marker[..^1] + (marker[^1] == '0' ? '1' : '0');
            Marker(marker);
            branches.Flush();
            if (scenario == "evicted-record")
            {
                var stored = Rows(log).Single(row => log.GetName(row) == "__transactions:transition:" + record.Serialize());
                log.RemoveName(stored);
                log.Delete(new[] { stored }, null);
                log.Flush();
                tx.Recover();
                Assert.DoesNotContain(tx.Log, t => t.Sequence == record.Sequence);
            }
            var before = Rows(branches).Select(branches.GetName).Order().ToArray();
            if (scenario is "valid" or "evicted-record")
            {
                using var recovered = new VersionControlDecorator(tx, branches);
                Assert.Equal("original", recovered.CurrentBranch);
                Assert.Equal(original, recovered.CurrentSequence);
                Assert.Equal(11u, recovered.GetSource(id));
                Assert.Equal(attempted, tx.LastLoggedSequence);
                Assert.DoesNotContain(Rows(branches), row => branches.GetName(row)?.StartsWith("__vc:move:", StringComparison.Ordinal) == true);
                recovered.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                Assert.Equal(attempted + 1, recovered.CurrentSequence);
            }
            else
            {
                Assert.Throws<InvalidDataException>(() => new VersionControlDecorator(tx, branches));
                Assert.Equal(21u, data.GetSource(id));
                Assert.Equal(attempted, tx.LastLoggedSequence);
                Assert.Equal(before, Rows(branches).Select(branches.GetName).Order().ToArray());
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
