using Foundation.Data.Doublets.Cli.Engine;
using Microsoft.Data.Sqlite;
using System.Text.Json;
using System.Buffers.Binary;

namespace Foundation.Data.Doublets.Cli.Tests;

// Contracts written before the adapter; real provider, finite data and busy retries.
public sealed class SqliteCommitLogTests : IDisposable
{
    private readonly string _directory = Path.Combine(Path.GetTempPath(), "clink-sqlite-" + Guid.NewGuid());
    public SqliteCommitLogTests() => Directory.CreateDirectory(_directory);
    public void Dispose() => Directory.Delete(_directory, true);
    private string PathFor(string name = "log") => Path.Combine(_directory, name + ".db");
    private static SqliteConnection Connection(string path)
    {
        var db = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = path, Pooling = false, DefaultTimeout = 1 }.ToString());
        db.Open(); return db;
    }
    [System.Diagnostics.CodeAnalysis.SuppressMessage("Reliability", "CA2000", Justification = "SqliteCommitLog owns and disposes the dedicated connection, including constructor failure.")]
    private static SqliteCommitLog Open(string path, SqliteLogOptions? options = null) => new(Connection(path), options);
    [System.Diagnostics.CodeAnalysis.SuppressMessage("Reliability", "CA2000", Justification = "SqliteFaultConnection owns and disposes its actual provider connection.")]
    private static SqliteFaultConnection Wrap(string path) => new(Connection(path));
    private static void Exec(SqliteConnection db, string sql) { using var command = db.CreateCommand(); command.CommandText = sql; command.CommandTimeout = 1; command.ExecuteNonQuery(); }
    private static long Count(SqliteConnection db)
    { using var command = db.CreateCommand(); command.CommandText = "SELECT COUNT(*) FROM link_cli_log"; command.CommandTimeout = 1; return (long)command.ExecuteScalar()!; }
    [Fact]
    public void IndependentBlobVectorsStageCommitAndRollbackOnDispose()
    {
        using var fixtures = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "sqlite-log-vectors.json")));
        var records = fixtures.RootElement.GetProperty("records").EnumerateArray().Select(row => row.GetProperty("text").GetString()!).ToArray();
        var path = PathFor(); using var log = Open(path); using var reader = Connection(path);
        Assert.True(log.SupportsDurableFlush); Assert.Empty(log.Read());
        foreach (var record in records) log.Append(record);
        Assert.Equal(records, log.Read()); Assert.Equal(0, Count(reader));
        foreach (var invalid in new[] { "one\ntwo", "\r", "\ud800", "\udfff" }) Assert.ThrowsAny<ArgumentException>(() => log.Append(invalid));
        log.Flush(); Assert.Equal(records.Length, Count(reader));
        using (var command = reader.CreateCommand())
        {
            command.CommandText = "SELECT record FROM link_cli_log ORDER BY sequence"; command.CommandTimeout = 1; using var rows = command.ExecuteReader();
            foreach (var row in fixtures.RootElement.GetProperty("records").EnumerateArray())
            { Assert.True(rows.Read()); Assert.Equal(row.GetProperty("utf8Hex").GetString(), Convert.ToHexString((byte[])rows.GetValue(0)).ToLowerInvariant()); }
        }
        log.Append("unflushed"); log.Dispose(); using var reopened = Open(path); Assert.Equal(records, reopened.Read());
        reopened.Append("following"); reopened.Flush(); Assert.Throws<ObjectDisposedException>(() => log.Read()); log.Dispose();
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void StaleOwnersFenceAllSubsequentOperations(bool read)
    {
        var path = PathFor(); using var winner = Open(path); using var stale = Open(path); winner.Append("winner"); winner.Flush();
        Assert.Contains("stale", Assert.ThrowsAny<IOException>(() => { if (read) stale.Read(); else stale.Append("stale"); }).Message, StringComparison.OrdinalIgnoreCase);
        Assert.ThrowsAny<IOException>(() => stale.Read()); Assert.ThrowsAny<IOException>(() => stale.Append("late")); Assert.ThrowsAny<IOException>(() => stale.Flush());
        Assert.Equal(new[] { "winner" }, winner.Read());
    }
    [Fact]
    public void BusyWriterAndPendingCheckpointDoNotPublish()
    {
        var path = PathFor(); using var winner = Open(path); using var competitor = Open(path); winner.Append("winner");
        Assert.Throws<InvalidOperationException>(() => winner.Checkpoint()); Assert.ThrowsAny<IOException>(() => competitor.Append("competing")); Assert.ThrowsAny<IOException>(() => competitor.Flush());
        winner.Flush(); Assert.Equal(new[] { "winner" }, winner.Read());
    }
    [Fact]
    public void ReaderPinsBlockCheckpointUntilReleased()
    {
        var path = PathFor(); using var log = Open(path, new() { AutoCheckpointPages = 0 }); using var reader = Connection(path);
        log.Append("one"); log.Flush(); Exec(reader, "BEGIN"); Assert.Equal(1, Count(reader)); log.Append("two"); log.Flush();
        var blocked = log.Checkpoint(SqliteCheckpointMode.Truncate); Assert.Equal(1, blocked.Busy); Assert.True(blocked.LogFrames > blocked.CheckpointedFrames);
        Exec(reader, "COMMIT"); var done = log.Checkpoint(SqliteCheckpointMode.Truncate); Assert.Equal(0, done.Busy); Assert.Equal(0, done.LogFrames);
        foreach (var mode in Enum.GetValues<SqliteCheckpointMode>()) Assert.Equal(0, log.Checkpoint(mode).Busy);
    }
    [Fact]
    public void ForeignSchemaSequenceHoleAndMalformedUtf8AreRejected()
    {
        var path = PathFor("foreign"); using (var db = Connection(path)) Exec(db, "CREATE TABLE keep(value TEXT); INSERT INTO keep VALUES ('keep')");
        Assert.ThrowsAny<IOException>(() => Open(path));
        using (var db = Connection(path)) { using var command = db.CreateCommand(); command.CommandText = "SELECT value FROM keep"; Assert.Equal("keep", command.ExecuteScalar()); }
        path = PathFor("hole"); using (var log = Open(path)) { log.Append("one"); log.Flush(); }
        using (var db = Connection(path)) Exec(db, "INSERT INTO link_cli_log VALUES (3, x'686f6c65')"); Assert.ThrowsAny<IOException>(() => Open(path));
        path = PathFor("utf8"); Open(path).Dispose(); using (var db = Connection(path)) Exec(db, "INSERT INTO link_cli_log VALUES (1, x'ff')");
        using var malformed = Open(path); Assert.ThrowsAny<IOException>(() => malformed.Read()); Assert.ThrowsAny<IOException>(() => malformed.Flush());
    }
    [Theory]
    [InlineData(StateStructure.Flat, RevisionEncoding.Snapshot)]
    [InlineData(StateStructure.Flat, RevisionEncoding.Changes)]
    [InlineData(StateStructure.Shared, RevisionEncoding.Snapshot)]
    [InlineData(StateStructure.Shared, RevisionEncoding.Changes)]
    public void EngineGroupsAndPinsReopenAcrossStrategies(StateStructure structure, RevisionEncoding encoding)
    {
        var path = PathFor(); var options = new EngineOptions { StateStructure = structure, RevisionEncoding = encoding, HistoryLimit = null, Isolation = IsolationLevel.Snapshot };
        using (var log = Open(path))
        {
            var engine = new LinksEngine<ulong>(log, options); var pin = engine.Snapshot(); using var a = engine.Begin(); using var b = engine.Begin();
            a.EnsureCreated(1); a.SetName(1, "one 雪\0😀"); b.EnsureCreated(2); Assert.Equal(new[] { 1UL, 2UL }, engine.CommitGroup(new[] { a, b }).Select(r => r.Revision!.Value)); Assert.Empty(pin.Rows);
        }
        using var reopened = Open(path); var next = new LinksEngine<ulong>(reopened, options); Assert.Equal(2, next.Snapshot().Rows.Count); Assert.Equal("one 雪\0😀", next.Snapshot().Rows[1].Name);
        using var following = next.Begin(); Assert.Equal(3UL, following.CreateLink(0, 0)); following.Commit();
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void CommitFailuresResolveOnlyTheActualCommittedPrefix(bool after)
    {
        var path = PathFor(); using (var connection = Wrap(path))
        using (var log = new SqliteCommitLog(connection))
        {
            log.Append("before"); log.Flush(); log.Append("uncertain");
            connection.Fault = (sql, completed) => { if (sql == "COMMIT" && completed == after) throw new IOException("injected commit failure"); };
            Assert.Contains("injected", Assert.Throws<IOException>(() => log.Flush()).Message);
            Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append("retry")); Assert.Throws<IOException>(() => log.Flush()); Assert.Throws<IOException>(() => log.Checkpoint());
        }
        using var reopened = Open(path); Assert.Equal(after ? new[] { "before", "uncertain" } : new[] { "before" }, reopened.Read()); reopened.Append("following"); reopened.Flush();
    }
    [Fact]
    public void FiniteSqliteFullPreservesPriorCommitAndAllowsFollowingWrites()
    {
        var path = PathFor(); using (var connection = Connection(path))
        using (var log = new SqliteCommitLog(connection))
        {
            log.Append("durable"); log.Flush(); using var pages = connection.CreateCommand(); pages.CommandText = "PRAGMA page_count"; pages.CommandTimeout = 1;
            Exec(connection, $"PRAGMA max_page_count={pages.ExecuteScalar()}");
            Assert.Contains("full", Assert.Throws<IOException>(() => log.Append(new string('x', 256 * 1024))).Message, StringComparison.OrdinalIgnoreCase);
            Assert.Throws<IOException>(() => log.Flush());
        }
        using var reopened = Open(path); Assert.Equal(new[] { "durable" }, reopened.Read()); reopened.Append("following"); reopened.Flush();
    }
    [Fact]
    public void SettingDriftAndInvalidOptionsFailBeforePublishing()
    {
        var path = PathFor(); using (var connection = Connection(path))
        using (var log = new SqliteCommitLog(connection))
        {
            Exec(connection, "PRAGMA synchronous=NORMAL"); Assert.Throws<IOException>(() => log.Append("unsafe")); Assert.Throws<IOException>(() => log.Flush());
        }
        using var reopened = Open(path); Assert.Empty(reopened.Read());
        foreach (var options in new[] { new SqliteLogOptions { BusyTimeoutMs = -1 }, new SqliteLogOptions { AutoCheckpointPages = -1 }, new SqliteLogOptions { CommandTimeoutSeconds = 0 } })
        { using var connection = Connection(path); Assert.Throws<ArgumentOutOfRangeException>(() => new SqliteCommitLog(connection, options)); Assert.Equal(System.Data.ConnectionState.Closed, connection.State); }
        Assert.Throws<ArgumentOutOfRangeException>(() => reopened.Checkpoint((SqliteCheckpointMode)99)); reopened.Append("following"); reopened.Flush();
    }
    [Fact]
    public void ConstructorAndCloseFaultsPreserveOriginalFailureAndPermitCleanupRetry()
    {
        var path = PathFor(); using (var connection = Wrap(path))
        {
            connection.Fault = (sql, _) => { if (sql is "COMMIT" or "CLOSE") throw new IOException(sql); };
            Assert.Equal("COMMIT", Assert.Throws<IOException>(() => new SqliteCommitLog(connection)).Message);
            connection.Fault = null;
        }
        using var retry = Wrap(path); var log = new SqliteCommitLog(retry); log.Append("pending");
        retry.Fault = (sql, _) => { if (sql == "CLOSE") throw new IOException("retry close"); };
        Assert.Throws<IOException>(() => log.Dispose()); retry.Fault = null; log.Dispose(); log.Dispose();
        using var reopened = Open(path); Assert.Empty(reopened.Read());
    }
    private static byte[] SharedBytes(string path)
    {
        using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        using var bytes = new MemoryStream(); file.CopyTo(bytes); return bytes.ToArray();
    }
    [Fact]
    public void CopiedWalTornFrameAndChecksumDamageRecoverOnlyCheckpointedRecords()
    {
        var source = PathFor("source"); using var log = Open(source, new() { AutoCheckpointPages = 0 });
        log.Append("checkpointed"); log.Flush(); log.Checkpoint(SqliteCheckpointMode.Truncate); log.Append("new committed frame"); log.Flush();
        var wal = SharedBytes(source + "-wal"); Assert.True(wal.Length >= 56);
        int pageSize = checked((int)BinaryPrimitives.ReadUInt32BigEndian(wal.AsSpan(8, 4))); Assert.InRange(pageSize, 512, 65536);
        int frameSize = 24 + pageSize; Assert.Equal(0, (wal.Length - 32) % frameSize); int frame = wal.Length - frameSize;
        Assert.NotEqual(0U, BinaryPrimitives.ReadUInt32BigEndian(wal.AsSpan(frame + 4, 4)));
        foreach (var damage in new[] { "control", "torn", "checksum" })
        {
            var path = PathFor(damage); File.WriteAllBytes(path, SharedBytes(source)); var bytes = (byte[])wal.Clone();
            if (damage == "torn") Array.Resize(ref bytes, bytes.Length - pageSize / 2);
            else if (damage == "checksum") bytes[frame + 24] ^= 1;
            File.WriteAllBytes(path + "-wal", bytes);
            using var recovered = Open(path); Assert.Equal(damage == "control" ? new[] { "checkpointed", "new committed frame" } : new[] { "checkpointed" }, recovered.Read());
            recovered.Append("following"); recovered.Flush();
            using var check = Connection(path); using var command = check.CreateCommand(); command.CommandText = "PRAGMA quick_check"; command.CommandTimeout = 1; Assert.Equal("ok", command.ExecuteScalar());
        }
    }
    [Fact]
    public void EveryInvalidFixtureSchemaAndUtf8PayloadFailsClosed()
    {
        using var fixtures = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "sqlite-log-vectors.json")));
        int index = 0;
        foreach (var schema in fixtures.RootElement.GetProperty("invalidSchemas").EnumerateArray())
        {
            var path = PathFor($"schema-{index++}"); using (var db = Connection(path)) Exec(db, $"{schema.GetString()}; PRAGMA application_id=1280199498; PRAGMA user_version=1");
            Assert.Throws<IOException>(() => Open(path));
        }
        foreach (var hex in fixtures.RootElement.GetProperty("invalidUtf8Hex").EnumerateArray())
        {
            var path = PathFor($"utf8-{index++}"); Open(path).Dispose(); using (var db = Connection(path)) Exec(db, $"INSERT INTO link_cli_log VALUES (1,x'{hex.GetString()}')");
            using var log = Open(path); Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append("late"));
        }
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void SqliteReplicationAndDeltaPreserveExactFollowingCommits(bool delta)
    {
        var primary = PathFor("primary"); var follower = PathFor("follower");
        static void Commit(ICommitLog log)
        {
            var db = new LinksEngine<ulong>(log, new() { RevisionEncoding = RevisionEncoding.Changes, RequireDurableFlush = true });
            int count = db.Snapshot().Rows.Count; using var tx = db.Begin(); tx.CreateLink(0, 0); tx.Commit(); Assert.Equal(count + 1, db.Snapshot().Rows.Count);
        }
        using (var a = Open(primary))
        using (var b = Open(follower))
        {
            ICommitLog replicated = new ReplicatedCommitLog(a, new[] { b }, 2); if (delta) replicated = new DeltaCommitLog(replicated); Commit(replicated);
            Assert.Equal(a.Read(), b.Read());
        }
        foreach (var path in new[] { primary, follower }) { using var log = Open(path); Commit(delta ? new DeltaCommitLog(log) : log); }
        using var one = Open(primary); using var two = Open(follower); Assert.Equal(one.Read(), two.Read());
    }
    [Theory]
    [InlineData("SELECT 1")]
    [InlineData("SELECT 'invalid'")]
    [InlineData("SELECT '3.51.2'")]
    public void UnsupportedRuntimeResponsesCloseTheOwnedConnection(string response)
    {
        var path = PathFor(); using var db = Wrap(path);
        db.Rewrite = sql => sql == "SELECT sqlite_version()" ? response : sql;
        Assert.Contains("3.51.3", Assert.Throws<IOException>(() => new SqliteCommitLog(db)).Message);
        Assert.Equal(System.Data.ConnectionState.Closed, db.State);
        using var reopened = Open(path); reopened.Append("following"); reopened.Flush();
    }
    [Fact]
    public void AClosedConnectionIsRejectedAndANewerRuntimeResponseIsAccepted()
    {
        using var closed = new SqliteConnection(); Assert.Throws<ArgumentException>(() => new SqliteCommitLog(closed));
        using var db = Wrap(PathFor()); db.Rewrite = sql => sql == "SELECT sqlite_version()" ? "SELECT '4.0.0'" : sql;
        using var log = new SqliteCommitLog(db); log.Append("future compatible runtime"); log.Flush(); Assert.Single(log.Read());
    }
    [Theory]
    [InlineData("PRAGMA journal_mode=WAL", "SELECT 'delete'")]
    [InlineData("PRAGMA journal_mode=WAL", "SELECT NULL")]
    [InlineData("PRAGMA busy_timeout", "SELECT 1")]
    [InlineData("PRAGMA wal_autocheckpoint", "SELECT 0")]
    public void IgnoredRequiredSettingsRejectAndReleaseTheConnection(string query, string response)
    {
        var path = PathFor(); using var db = Wrap(path); db.Rewrite = sql => sql == query ? response : sql;
        Assert.Throws<IOException>(() => new SqliteCommitLog(db)); Assert.Equal(System.Data.ConnectionState.Closed, db.State);
        using var reopened = Open(path); Assert.Empty(reopened.Read());
    }
    [Fact]
    public void MissingSummaryAndMaintenanceRowsFenceWithoutChangingCommittedRecords()
    {
        var path = PathFor(); using (var db = Wrap(path))
        {
            db.Rewrite = sql => sql.StartsWith("SELECT COUNT(*)", StringComparison.Ordinal) ? sql + " WHERE 0 GROUP BY sequence" : sql;
            Assert.Throws<IOException>(() => new SqliteCommitLog(db));
        }
        using (var db = Wrap(path))
        using (var log = new SqliteCommitLog(db))
        {
            log.Append("durable"); log.Flush(); db.Rewrite = sql => sql.StartsWith("PRAGMA wal_checkpoint(", StringComparison.Ordinal) ? "SELECT 0,0,0 WHERE 0" : sql;
            Assert.Throws<IOException>(() => log.Checkpoint()); Assert.Throws<IOException>(() => log.Append("late"));
        }
        using var recovered = Open(path); Assert.Equal(new[] { "durable" }, recovered.Read()); recovered.Append("following"); recovered.Flush();
    }
    [Fact]
    public void VirtualExhaustedSummaryRejectsBeforeComponentIoAndRemainsUsable()
    {
        // A finite provider response models i64 exhaustion without allocating
        // billions of rows. Production SQL and schemas are never altered.
        using var db = Wrap(PathFor()); db.Rewrite = sql => sql.StartsWith("SELECT COUNT(*)", StringComparison.Ordinal) ? "SELECT 9223372036854775807,1,9223372036854775807" : sql;
        using var log = new SqliteCommitLog(db); db.Fault = (_, _) => throw new IOException("unexpected component IO");
        Assert.Throws<OverflowException>(() => log.Append("exhausted")); log.Flush();
        db.Fault = null; Assert.Equal(0, log.Checkpoint().Busy);
    }
    [Theory]
    [InlineData("SELECT 'delete'")]
    [InlineData("SELECT NULL")]
    public void JournalModeDriftFencesAndReopeningPreservesThePriorPrefix(string response)
    {
        var path = PathFor(); using (var db = Wrap(path))
        using (var log = new SqliteCommitLog(db))
        {
            log.Append("prior"); log.Flush(); db.Rewrite = sql => sql == "PRAGMA journal_mode" ? response : sql;
            Assert.Throws<IOException>(() => log.Append("unsafe")); Assert.Throws<IOException>(() => log.Flush());
        }
        using var recovered = Open(path); Assert.Equal(new[] { "prior" }, recovered.Read()); recovered.Append("following"); recovered.Flush();
    }
    [Theory]
    [InlineData("PRAGMA application_id")]
    [InlineData("PRAGMA user_version")]
    [InlineData("PRAGMA busy_timeout")]
    [InlineData("PRAGMA wal_autocheckpoint")]
    public void MissingNumericProviderRowsAreNeverInterpretedAsZero(string query)
    {
        var path = PathFor(); using var db = Wrap(path);
        db.Rewrite = sql => sql == query ? "SELECT 0 WHERE 0" : sql;
        Assert.Throws<IOException>(() => new SqliteCommitLog(db, new() { AutoCheckpointPages = 0 }));
        Assert.Equal(System.Data.ConnectionState.Closed, db.State);
        using var reopened = Open(path); reopened.Append("following"); reopened.Flush(); Assert.Single(reopened.Read());
    }
}
