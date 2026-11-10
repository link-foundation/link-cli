using System.Data;
using System.Data.Common;
using System.Globalization;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record SqliteLogOptions
{
    public int BusyTimeoutMs { get; init; }
    public int AutoCheckpointPages { get; init; } = 1000;
    /// <summary>Positive finite provider retry limit. Zero would mean unlimited
    /// retries in Microsoft.Data.Sqlite, independently of SQLite busy_timeout.</summary>
    public int CommandTimeoutSeconds { get; init; } = 1;
}
public enum SqliteCheckpointMode { Passive, Full, Restart, Truncate }
public readonly record struct SqliteCheckpointResult(long Busy, long LogFrames, long CheckpointedFrames);

/// <summary>Optional physical SQLite journal using an owned, dedicated, already
/// open DbConnection. No SQLite provider is required by this library. Appends
/// share BEGIN IMMEDIATE until Flush COMMITs. A stale owner or component failure
/// fences this log until disposal/reopening discovers its committed prefix.</summary>
public sealed class SqliteCommitLog : ICommitLog, IDisposable
{
    private const long ApplicationId = 0x4c4e4b4a;
    private const string Schema = "CREATE TABLE link_cli_log (sequence INTEGER PRIMARY KEY CHECK(sequence>0), record BLOB NOT NULL CHECK(typeof(record)='blob'))";
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private readonly DbConnection _connection;
    private readonly SqliteLogOptions _options;
    private long _tail;
    private bool _pending, _poisoned, _closed;
    public bool SupportsDurableFlush => true;

    public SqliteCommitLog(DbConnection connection, SqliteLogOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(connection);
        _connection = connection; _options = options ?? new();
        try
        {
            if (_options.BusyTimeoutMs < 0 || _options.AutoCheckpointPages < 0 || _options.CommandTimeoutSeconds <= 0)
                throw new ArgumentOutOfRangeException(nameof(options), "SQLite options require nonnegative intervals and a positive command timeout.");
            if (_connection.State != ConnectionState.Open) throw new ArgumentException("The owned SQLite connection must already be open.", nameof(connection));
            Initialize();
        }
        catch (Exception error)
        {
            Rollback();
            try { _connection.Dispose(); } catch { /* Preserve the constructor failure. */ }
            _closed = true;
            if (error is ArgumentException or IOException) throw;
            throw Failure(error);
        }
    }
    private void Initialize()
    {
        if (Scalar("SELECT sqlite_version()") is not string text || !Version.TryParse(text, out var version) || version < new Version(3, 51, 3))
            throw new InvalidDataException("SQLite >= 3.51.3 is required.");
        Identity(); // Reject a foreign database before changing its journal mode.
        Exec($"PRAGMA busy_timeout={_options.BusyTimeoutMs.ToString(CultureInfo.InvariantCulture)}");
        if (Scalar("PRAGMA journal_mode=WAL") is not "wal") throw new InvalidDataException("SQLite journal requires file-backed WAL.");
        Exec($"PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint={_options.AutoCheckpointPages.ToString(CultureInfo.InvariantCulture)}");
        Settings();
        if (Number("PRAGMA busy_timeout") != _options.BusyTimeoutMs || Number("PRAGMA wal_autocheckpoint") != _options.AutoCheckpointPages)
            throw new InvalidDataException("SQLite options were not applied.");
        Exec("BEGIN IMMEDIATE"); _pending = true;
        if (Identity()) Exec($"{Schema}; PRAGMA application_id={ApplicationId.ToString(CultureInfo.InvariantCulture)}; PRAGMA user_version=1");
        using (var command = Command("SELECT COUNT(*), COALESCE(MIN(sequence),0), COALESCE(MAX(sequence),0) FROM link_cli_log"))
        using (var rows = command.ExecuteReader())
        {
            if (!rows.Read()) throw new InvalidDataException("Missing journal sequence summary.");
            long count = rows.GetInt64(0), first = rows.GetInt64(1), last = rows.GetInt64(2);
            if (count < 0 || count != last || first != (count == 0 ? 0 : 1)) throw new InvalidDataException("Journal sequences must be contiguous from one.");
            _tail = last;
        }
        Exec("COMMIT"); _pending = false;
    }
    private DbCommand Command(string sql)
    {
        var command = _connection.CreateCommand(); command.CommandText = sql; command.CommandTimeout = _options.CommandTimeoutSeconds; return command;
    }
    private void Exec(string sql) { using var command = Command(sql); command.ExecuteNonQuery(); }
    private object? Scalar(string sql) { using var command = Command(sql); return command.ExecuteScalar(); }
    private long Number(string sql) => Convert.ToInt64(Scalar(sql) ?? throw new InvalidDataException("Missing SQLite integer result."), CultureInfo.InvariantCulture);
    private bool Identity()
    {
        long id = Number("PRAGMA application_id"), version = Number("PRAGMA user_version");
        using var command = Command("SELECT type, name, tbl_name, sql FROM sqlite_schema ORDER BY name"); using var rows = command.ExecuteReader();
        bool any = rows.Read();
        if (id == 0 && version == 0 && !any) return true;
        if (id != ApplicationId || version != 1 || !any || rows.GetString(0) != "table" || rows.GetString(1) != "link_cli_log" || rows.GetString(2) != "link_cli_log" || rows.IsDBNull(3) || rows.GetString(3) != Schema || rows.Read())
            throw new InvalidDataException("Foreign or unsupported journal schema.");
        return false;
    }
    private void Settings()
    {
        if (Scalar("PRAGMA journal_mode") is not "wal" || Number("PRAGMA synchronous") != 2)
            throw new InvalidDataException("SQLite WAL/FULL settings changed.");
    }
    private void Check()
    {
        ObjectDisposedException.ThrowIf(_closed, this);
        if (_poisoned) throw new IOException("Indeterminate journal outcome; dispose and reopen.");
    }
    private void Rollback()
    {
        if (!_pending) return;
        // COMMIT can throw after committing. Roll back only a remaining SQL
        // transaction; never erase committed rows or mask the original failure.
        try { Exec("ROLLBACK"); } catch { }
        _pending = false;
    }
    private static IOException Failure(Exception error) => new($"SQLite journal component failed: {error.Message}", error);
    private T Io<T>(Func<T> operation)
    {
        try { return operation(); }
        catch (Exception error)
        {
            _poisoned = true; Rollback();
            if (error is IOException) throw;
            throw Failure(error);
        }
    }
    public IReadOnlyList<string> Read()
    {
        Check();
        return Io<IReadOnlyList<string>>(() =>
        {
            using var command = Command("SELECT sequence, record FROM link_cli_log ORDER BY sequence"); using var rows = command.ExecuteReader();
            var records = new List<string>(); long previous = 0;
            while (rows.Read())
            {
                long current = rows.GetInt64(0);
                if (previous == long.MaxValue || current != previous + 1) throw new InvalidDataException("Journal sequences must be contiguous from one.");
                var record = Utf8.GetString(rows.GetFieldValue<byte[]>(1));
                if (record.Contains('\r') || record.Contains('\n')) throw new InvalidDataException("Journal record contains a physical line break.");
                records.Add(record); previous = current;
            }
            if (previous != _tail) throw new InvalidDataException("Stale journal owner: sequence tail changed.");
            return records;
        });
    }
    public void Append(string record)
    {
        Check(); ArgumentNullException.ThrowIfNull(record);
        if (record.Contains('\r') || record.Contains('\n')) throw new ArgumentException("Journal records must not contain line breaks.", nameof(record));
        var bytes = Utf8.GetBytes(record); // Reject invalid Unicode before component I/O.
        if (_tail == long.MaxValue) throw new OverflowException("Journal sequence exhausted.");
        long next = _tail + 1;
        Io(() =>
        {
            if (!_pending)
            {
                Exec("BEGIN IMMEDIATE"); _pending = true; Settings();
                if (Number("SELECT COALESCE(MAX(sequence),0) FROM link_cli_log") != _tail) throw new InvalidDataException("Stale journal owner: sequence tail changed.");
            }
            using var command = Command("INSERT INTO link_cli_log (sequence, record) VALUES ($sequence, $record)");
            var sequence = command.CreateParameter(); sequence.ParameterName = "$sequence"; sequence.DbType = DbType.Int64; sequence.Value = next; command.Parameters.Add(sequence);
            var payload = command.CreateParameter(); payload.ParameterName = "$record"; payload.DbType = DbType.Binary; payload.Value = bytes; command.Parameters.Add(payload);
            command.ExecuteNonQuery(); _tail = next; return 0;
        });
    }
    public void Flush()
    {
        Check(); if (!_pending) return;
        Io(() => { Settings(); Exec("COMMIT"); _pending = false; return 0; });
    }
    /// <summary>Returns actual physical checkpoint counters. Does not remove
    /// logical records. Pending appends reject without poisoning the writer.</summary>
    public SqliteCheckpointResult Checkpoint(SqliteCheckpointMode mode = SqliteCheckpointMode.Passive)
    {
        Check();
        if (!Enum.IsDefined(mode)) throw new ArgumentOutOfRangeException(nameof(mode));
        if (_pending) throw new InvalidOperationException("Cannot checkpoint pending appends.");
        return Io(() =>
        {
            Settings(); using var command = Command($"PRAGMA wal_checkpoint({mode.ToString().ToUpperInvariant()})"); using var rows = command.ExecuteReader();
            if (!rows.Read()) throw new InvalidDataException("Missing SQLite checkpoint result.");
            return new SqliteCheckpointResult(rows.GetInt64(0), rows.GetInt64(1), rows.GetInt64(2));
        });
    }
    public void Dispose()
    {
        if (_closed) return;
        Rollback(); _connection.Dispose(); _closed = true; // A failed provider close can be retried.
    }
}
