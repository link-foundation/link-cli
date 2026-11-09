using System.Text;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Damaged journals: what Check reports, what Recover keeps, and that a
/// repaired file journal opens at the recovered revision. Twin of Rust's
/// engine_recovery_tests.</summary>
public class EngineJournalRecoveryTests
{
    // Revision k holds links 1..=k, each pointing at itself.
    private static IReadOnlyList<string> Journal(int commits, RevisionEncoding encoding = RevisionEncoding.Snapshot)
    {
        var log = new MemoryCommitLog();
        var engine = new LinksEngine<ulong>(log, new() { RevisionEncoding = encoding });
        for (int commit = 0; commit < commits; commit++)
        {
            using var tx = engine.Begin();
            var id = tx.CreateLink(0, 0);
            tx.UpdateLink(id, id, id);
            tx.Commit();
        }
        return log.Read();
    }

    private static ulong[] Expected(ulong revision) => Enumerable.Range(1, (int)revision).Select(i => (ulong)i).ToArray();

    private static void AssertState(LinksEngine<ulong> engine, ulong revision)
    {
        var snapshot = engine.Snapshot();
        Assert.Equal(revision, snapshot.Revision);
        Assert.Equal(Expected(revision), snapshot.Rows.Keys.OrderBy(id => id).ToArray());
        Assert.All(snapshot.Rows, row => Assert.True(row.Value.Link.Source == row.Key && row.Value.Link.Target == row.Key));
    }

    private static LinksEngine<ulong> Open(IEnumerable<string> records)
    {
        var log = new MemoryCommitLog();
        foreach (var record in records) log.Append(record);
        return new(log);
    }

    [Fact]
    public void AnIntactJournalReportsEveryRecord()
    {
        foreach (var encoding in Enum.GetValues<RevisionEncoding>())
        {
            var check = EngineJournal<ulong>.Check(Journal(4, encoding));
            Assert.True(check.IsIntact);
            Assert.Equal((4, 4, 4ul, 0), (check.ValidRecords, check.TotalRecords, check.Revision, check.DiscardedRecords));
        }
        var empty = EngineJournal<ulong>.Check(Array.Empty<string>());
        Assert.True(empty.IsIntact);
        Assert.Equal(0ul, empty.Revision);
    }

    [Fact]
    public void DamageAnywhereKeepsExactlyTheRecordsBeforeIt()
    {
        foreach (var encoding in Enum.GetValues<RevisionEncoding>())
        {
            var entries = Journal(3, encoding);
            for (int record = 0; record < entries.Count; record++)
                for (int position = 0; position < entries[record].Length; position++)
                {
                    var damaged = entries.ToArray();
                    var chars = damaged[record].ToCharArray();
                    chars[position] = (char)(chars[position] ^ 1);
                    damaged[record] = new string(chars);
                    var check = EngineJournal<ulong>.Check(damaged);
                    Assert.False(check.IsIntact, $"{encoding} flip at {record}:{position} went unnoticed");
                    Assert.Equal(record, check.Damage!.Record);
                    Assert.Equal((record, (ulong)record, entries.Count - record), (check.ValidRecords, check.Revision, check.DiscardedRecords));
                    var recovery = EngineJournal<ulong>.Recover(new[] { damaged });
                    AssertState(Open(recovery.Entries), (ulong)record);
                }
        }
    }

    [Fact]
    public void MissingReorderedAndForeignRecordsEndTheValidPrefix()
    {
        var entries = Journal(4);
        var missing = entries.ToList();
        missing.RemoveAt(1);
        Assert.Equal((1, 1ul), (EngineJournal<ulong>.Check(missing).ValidRecords, EngineJournal<ulong>.Check(missing).Revision));
        var swapped = entries.ToArray();
        (swapped[2], swapped[3]) = (swapped[3], swapped[2]);
        Assert.Equal(2, EngineJournal<ulong>.Check(swapped).ValidRecords);
        var foreign = entries.ToList();
        foreign.Insert(2, "not a journal record");
        var check = EngineJournal<ulong>.Check(foreign);
        Assert.Equal((2, 5), (check.ValidRecords, check.TotalRecords));
        // A wider-address record is refused by a narrower reader, not truncated.
        var log = new MemoryCommitLog();
        var wide = new LinksEngine<ulong>(log);
        using (var tx = wide.Begin()) { var id = tx.CreateLink(0, 0); tx.UpdateLink(id, (ulong)uint.MaxValue + 1, id); tx.Commit(); }
        Assert.Equal(0, EngineJournal<uint>.Check(log.Read()).ValidRecords);
        Assert.True(EngineJournal<ulong>.Check(log.Read()).IsIntact);
    }

    [Fact]
    public void SelectedConstraintsArePartOfTheCheck()
    {
        var log = new MemoryCommitLog();
        var engine = new LinksEngine<ulong>(log);
        using (var tx = engine.Begin()) { tx.CreateLink(5, 5); tx.Commit(); }
        Assert.True(EngineJournal<ulong>.Check(log.Read()).IsIntact);
        var check = EngineJournal<ulong>.Check(log.Read(), new() { Constraints = new[] { EngineConstraint.References } });
        Assert.Equal(0, check.ValidRecords);
        Assert.Contains("dangling", check.Damage!.Reason);
        // Options that do not change what replays are accepted.
        Assert.True(EngineJournal<ulong>.Check(log.Read(), new() { RequireDurableFlush = true, HistoryLimit = null }).IsIntact);
    }

    [Fact]
    public void ACheckpointIsOnlyAcceptedFirst()
    {
        var checkpoint = Open(Journal(3)).ExportCheckpoint();
        Assert.StartsWith("engine3|", checkpoint[0]);
        var check = EngineJournal<ulong>.Check(checkpoint);
        Assert.Equal((1, 3ul), (check.ValidRecords, check.Revision));
        var late = Journal(3).Append(checkpoint[0]).ToArray();
        Assert.Equal(3, EngineJournal<ulong>.Check(late).ValidRecords);
    }

    [Fact]
    public void AnIntactCopyRestoresWhatTheDamagedOneLost()
    {
        var entries = Journal(5, RevisionEncoding.Changes);
        var damaged = entries.ToArray();
        damaged[2] = damaged[2][..10] + "zz" + damaged[2][12..];
        var lagging = entries.Take(3).ToArray();
        var recovery = EngineJournal<ulong>.Recover(new IReadOnlyList<string>[] { damaged, lagging, entries });
        Assert.Equal((2, 5ul), (recovery.Source, recovery.Revision));
        Assert.Equal(entries, recovery.Entries);
        Assert.Equal(new[] { 2, 3, 5 }, recovery.Checks.Select(check => check.ValidRecords));
        // Ties keep the earliest copy, normally the primary.
        Assert.Equal(0, EngineJournal<ulong>.Recover(new IReadOnlyList<string>[] { lagging, lagging }).Source);
    }

    [Fact]
    public void CopiesThatDisagreeAreRefused()
    {
        var log = new MemoryCommitLog();
        var engine = new LinksEngine<ulong>(log);
        foreach (var (source, target) in new[] { (0ul, 0ul), (1ul, 1ul) })
            using (var tx = engine.Begin()) { tx.CreateLink(source, target); tx.Commit(); }
        var error = Assert.Throws<InvalidOperationException>(() => EngineJournal<ulong>.Recover(new[] { Journal(3), log.Read() }));
        Assert.Contains("diverge", error.Message);
        Assert.Throws<ArgumentException>(() => EngineJournal<ulong>.Recover(Array.Empty<IReadOnlyList<string>>()));
    }

    private sealed class Directory : IDisposable
    {
        internal readonly string Path = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "journal-recovery-" + Guid.NewGuid().ToString("N"));
        internal Directory() => System.IO.Directory.CreateDirectory(Path);
        internal string File(string name) => System.IO.Path.Combine(Path, name);
        public void Dispose() => System.IO.Directory.Delete(Path, true);
    }

    private static void Write(string path, IEnumerable<string> entries)
        => File.WriteAllText(path, string.Concat(entries.Select(entry => entry + "\n")));

    private static ulong OpenRevision(string path)
    {
        using var log = new FileCommitLog(path);
        var engine = new LinksEngine<ulong>(log);
        AssertState(engine, engine.Snapshot().Revision);
        return engine.Snapshot().Revision;
    }

    [Fact]
    public void EveryDamagedByteOfAFileIsRepairedToItsValidPrefix()
    {
        using var directory = new Directory();
        var path = directory.File("links.journal");
        Write(path, Journal(3, RevisionEncoding.Changes));
        var pristine = File.ReadAllBytes(path);
        for (int position = 0; position < pristine.Length; position++)
        {
            var bytes = pristine.ToArray();
            bytes[position] = bytes[position] == 0xff ? (byte)0 : (byte)0xff;
            File.WriteAllBytes(path, bytes);
            // The final newline turns the last record into a torn tail, which
            // opening drops by design; every other byte is refused.
            if (position + 1 < pristine.Length)
                Assert.ThrowsAny<Exception>(() => { using var log = new FileCommitLog(path); _ = new LinksEngine<ulong>(log); });
            var repair = EngineJournal<ulong>.RepairFile(path, Array.Empty<string>());
            ulong line = (ulong)pristine.Take(position).Count(b => b == (byte)'\n');
            Assert.Equal(line, repair.Recovery.Revision);
            Assert.True(repair.Changed);
            Assert.Equal(bytes, File.ReadAllBytes(repair.Preserved!));
            File.Delete(repair.Preserved!);
            Assert.Equal(line, OpenRevision(path));
        }
    }

    [Fact]
    public void AReplicaFileRestoresTheLostSuffixAndEvidenceIsKept()
    {
        using var directory = new Directory();
        var path = directory.File("primary.journal");
        var replica = directory.File("replica.journal");
        var entries = Journal(4);
        Write(path, entries);
        Write(replica, entries);
        // A zeroed block, as a lost page write leaves behind.
        var bytes = File.ReadAllBytes(path);
        Array.Clear(bytes, bytes.Length / 2, 24);
        File.WriteAllBytes(path, bytes);
        var first = EngineJournal<ulong>.RepairFile(path, new[] { replica });
        Assert.Equal((1, 4ul), (first.Recovery.Source, first.Recovery.Revision));
        Assert.Equal(path + ".damaged-1", first.Preserved);
        Assert.Equal(4ul, OpenRevision(path));
        // A second damage does not overwrite the first evidence.
        File.WriteAllText(path, "garbage\n");
        Assert.Equal(path + ".damaged-2", EngineJournal<ulong>.RepairFile(path, new[] { replica }).Preserved);
        Assert.Equal(bytes, File.ReadAllBytes(path + ".damaged-1"));
        // An intact journal is left alone.
        var again = EngineJournal<ulong>.RepairFile(path, new[] { replica });
        Assert.False(again.Changed);
        Assert.Null(again.Preserved);
    }

    [Fact]
    public void TornTailsInvalidUtf8AndMissingFilesAreReadLeniently()
    {
        using var directory = new Directory();
        var path = directory.File("links.journal");
        Assert.Empty(EngineJournal<ulong>.ReadFileLossy(path));
        var missing = EngineJournal<ulong>.RepairFile(path, Array.Empty<string>());
        Assert.False(missing.Changed);
        Assert.Empty(missing.Recovery.Entries);
        var entries = Journal(2);
        var bytes = new List<byte>();
        bytes.AddRange(Encoding.UTF8.GetBytes(entries[0] + "\n\n" + entries[1] + "\n"));
        bytes.AddRange(new byte[] { 0xc3, 0x28 });
        bytes.AddRange(Encoding.UTF8.GetBytes(" torn"));
        File.WriteAllBytes(path, bytes.ToArray());
        Assert.Equal(entries, EngineJournal<ulong>.ReadFileLossy(path));
        var repair = EngineJournal<ulong>.RepairFile(path, Array.Empty<string>());
        Assert.True(repair.Recovery.Checks[0].IsIntact);
        Assert.True(repair.Changed);
        Assert.Equal(2ul, OpenRevision(path));
        // Invalid UTF-8 inside a complete record is damage, not a read error.
        var damaged = File.ReadAllBytes(path);
        damaged[entries[0].Length + 3] = 0xc3;
        File.WriteAllBytes(path, damaged);
        Assert.ThrowsAny<Exception>(() => { using var log = new FileCommitLog(path); log.Read(); });
        Assert.Equal(1ul, EngineJournal<ulong>.RepairFile(path, Array.Empty<string>()).Recovery.Revision);
    }

    [Fact]
    public void ARunningWriterBlocksRepair()
    {
        using var directory = new Directory();
        var path = directory.File("links.journal");
        using var writer = new FileCommitLog(path);
        var error = Assert.Throws<IOException>(() => EngineJournal<ulong>.RepairFile(path, Array.Empty<string>()));
        Assert.Contains("busy", error.Message);
    }

    [Fact]
    public void DivergentFileCopiesLeaveTheJournalUntouched()
    {
        using var directory = new Directory();
        var path = directory.File("links.journal");
        var other = directory.File("other.journal");
        Write(path, Journal(2));
        Write(other, Journal(2, RevisionEncoding.Changes));
        var before = File.ReadAllBytes(path);
        Assert.Throws<InvalidOperationException>(() => EngineJournal<ulong>.RepairFile(path, new[] { other }));
        Assert.Equal(before, File.ReadAllBytes(path));
    }
}
