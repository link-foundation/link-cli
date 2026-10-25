using System.Diagnostics;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

// Abrupt process loss leaves the OS running; this is not a power-loss model.
public class EngineSqliteExitTests
{
    private sealed class ExitLog(ICommitLog inner, string point) : ICommitLog
    {
        private int _appends;
        public bool SupportsDurableFlush => inner.SupportsDurableFlush;
        public IReadOnlyList<string> Read() => inner.Read();
        public void Append(string record)
        {
            if (point == "before-append" && _appends == 0) Environment.Exit(71);
            inner.Append(record); _appends++;
            if (point == "after-append" && _appends == 1) Environment.Exit(71);
        }
        public void Flush()
        {
            if (point == "before-flush") Environment.Exit(71);
            inner.Flush();
            if (point == "after-flush") Environment.Exit(71);
        }
    }

    [System.Diagnostics.CodeAnalysis.SuppressMessage("Reliability", "CA2000", Justification = "SqliteCommitLog owns and disposes the dedicated connection, including constructor failure.")]
    private static SqliteCommitLog Open(string path)
    {
        var connection = new Microsoft.Data.Sqlite.SqliteConnection(new Microsoft.Data.Sqlite.SqliteConnectionStringBuilder { DataSource = path, Pooling = false, DefaultTimeout = 1 }.ToString());
        connection.Open(); return new SqliteCommitLog(connection);
    }
    private static EngineOptions Options(RevisionEncoding encoding, StateStructure structure) => new()
    { Isolation = IsolationLevel.Snapshot, HistoryLimit = null, RequireDurableFlush = true, RevisionEncoding = encoding, StateStructure = structure };
    internal static int Child(string path, string point, bool delta, RevisionEncoding encoding = RevisionEncoding.Snapshot, StateStructure structure = StateStructure.Flat)
    {
        using var file = Open(path);
        ICommitLog log = new ExitLog(file, point);
        if (delta) log = new DeltaCommitLog(log);
        var engine = new LinksEngine<uint>(log, Options(encoding, structure));
        using var a = engine.Begin(); a.CreateLink(0, 0); a.SetName(1, "first 雪");
        using var b = engine.Begin(); b.CreateLink(1, 0); b.SetName(2, "second");
        Assert.Equal(new[] { new GroupCommitOutcome(GroupCommitStatus.Committed, 1), new GroupCommitOutcome(GroupCommitStatus.Committed, 2) }, engine.CommitGroup(new[] { a, b }));
        Environment.Exit(0); // Acknowledged control; deliberately skip disposal.
        return 0;
    }

    [Theory(Timeout = 90000)]
    [InlineData(false, RevisionEncoding.Snapshot, StateStructure.Flat)]
    [InlineData(true, RevisionEncoding.Snapshot, StateStructure.Flat)]
    [InlineData(false, RevisionEncoding.Snapshot, StateStructure.Shared)]
    [InlineData(true, RevisionEncoding.Snapshot, StateStructure.Shared)]
    [InlineData(false, RevisionEncoding.Changes, StateStructure.Flat)]
    [InlineData(true, RevisionEncoding.Changes, StateStructure.Flat)]
    [InlineData(false, RevisionEncoding.Changes, StateStructure.Shared)]
    [InlineData(true, RevisionEncoding.Changes, StateStructure.Shared)]
    public async Task AbruptSqliteGroupExitsRecoverExactPrefixesAndFollowingCommits(bool delta, RevisionEncoding encoding, StateStructure structure)
    {
        foreach (var (point, expected) in new[] { ("before-append", 0ul), ("after-append", 0ul), ("before-flush", 0ul), ("after-flush", 2ul), ("acknowledged", 2ul) })
        {
            var directory = Directory.CreateTempSubdirectory("engine-sqlite-exit-").FullName;
            var path = Path.Combine(directory, "journal");
            try
            {
                var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
                info.Environment["DOTNET_GCHeapHardLimit"] = "0x10000000";
                foreach (var argument in new[] { typeof(EngineSqliteExitTests).Assembly.Location, "sqlite-exit", path, point, delta.ToString(), encoding.ToString(), structure.ToString() }) info.ArgumentList.Add(argument);
                using var child = Process.Start(info)!;
                var stdout = child.StandardOutput.ReadToEndAsync(); var stderr = child.StandardError.ReadToEndAsync();
                using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                try { await child.WaitForExitAsync(deadline.Token); }
                catch (OperationCanceledException) { child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); await Task.WhenAll(stdout, stderr); throw new TimeoutException("Finite group-exit child expired."); }
                Assert.True(child.ExitCode == (point == "acknowledged" ? 0 : 71), $"{point}: {await stdout}\n{await stderr}");
                for (int reopen = 0; reopen < 2; reopen++)
                {
                    using var file = Open(path);
                    var selected = reopen == 0 ? (encoding == RevisionEncoding.Snapshot ? RevisionEncoding.Changes : RevisionEncoding.Snapshot) : encoding;
                    var engine = new LinksEngine<uint>(delta ? new DeltaCommitLog(file) : file, Options(selected, structure));
                    ulong revision = expected + (reopen == 0 ? 0ul : 1ul);
                    Assert.Equal(revision, engine.Snapshot().Revision); Assert.Equal((int)revision, engine.Snapshot().Rows.Count);
                    if (expected > 0) Assert.Equal("first 雪", engine.SnapshotAt(1).Rows[1].Name);
                    if (expected > 1) Assert.Equal("second", engine.SnapshotAt(2).Rows[2].Name);
                    if (reopen == 0)
                    {
                        // The first record already persists both reserved addresses.
                        using var next = engine.Begin(); Assert.Equal(expected == 0 ? 1u : 3u, next.CreateLink(0, 0)); Assert.Equal(expected + 1, next.Commit());
                    }
                }
            }
            finally { Directory.Delete(directory, recursive: true); }
        }
    }
}
