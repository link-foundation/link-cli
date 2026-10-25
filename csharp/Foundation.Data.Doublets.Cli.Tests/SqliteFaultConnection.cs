using System.Data;
using System.Data.Common;
using System.Diagnostics.CodeAnalysis;

namespace Foundation.Data.Doublets.Cli.Tests;

// Exercises the public provider-neutral seam while retaining actual SQLite I/O.
internal sealed class SqliteFaultConnection(DbConnection connection) : DbConnection
{
    public Action<string, bool>? Fault { get; set; }
    // Narrow response controls still execute real SQL against the owned provider.
    public Func<string, string>? Rewrite { get; set; }
    [AllowNull]
    public override string ConnectionString { get => connection.ConnectionString; set => connection.ConnectionString = value; }
    public override string Database => connection.Database;
    public override string DataSource => connection.DataSource;
    public override string ServerVersion => connection.ServerVersion;
    public override ConnectionState State => connection.State;
    public override void ChangeDatabase(string databaseName) => connection.ChangeDatabase(databaseName);
    public override void Close() => connection.Close();
    public override void Open() => connection.Open();
    protected override DbTransaction BeginDbTransaction(IsolationLevel isolationLevel) => connection.BeginTransaction(isolationLevel);
    protected override DbCommand CreateDbCommand() => new Command(connection.CreateCommand(), this);
    protected override void Dispose(bool disposing)
    {
        if (disposing) { Fault?.Invoke("CLOSE", false); connection.Dispose(); Fault?.Invoke("CLOSE", true); }
        base.Dispose(disposing);
    }
    private sealed class Command(DbCommand inner, SqliteFaultConnection owner) : DbCommand
    {
        [AllowNull]
        public override string CommandText { get => inner.CommandText; set => inner.CommandText = value is null ? null : owner.Rewrite?.Invoke(value) ?? value; }
        public override int CommandTimeout { get => inner.CommandTimeout; set => inner.CommandTimeout = value; }
        public override CommandType CommandType { get => inner.CommandType; set => inner.CommandType = value; }
        public override bool DesignTimeVisible { get => inner.DesignTimeVisible; set => inner.DesignTimeVisible = value; }
        public override UpdateRowSource UpdatedRowSource { get => inner.UpdatedRowSource; set => inner.UpdatedRowSource = value; }
        protected override DbConnection? DbConnection { get => inner.Connection; set => inner.Connection = value; }
        protected override DbParameterCollection DbParameterCollection => inner.Parameters;
        protected override DbTransaction? DbTransaction { get => inner.Transaction; set => inner.Transaction = value; }
        public override void Cancel() => inner.Cancel();
        public override void Prepare() => inner.Prepare();
        protected override DbParameter CreateDbParameter() => inner.CreateParameter();
        private T Execute<T>(Func<T> operation)
        {
            Assert.InRange(CommandTimeout, 1, int.MaxValue);
            owner.Fault?.Invoke(CommandText, false); var result = operation(); owner.Fault?.Invoke(CommandText, true); return result;
        }
        public override int ExecuteNonQuery() => Execute(inner.ExecuteNonQuery);
        public override object? ExecuteScalar() => Execute(inner.ExecuteScalar);
        protected override DbDataReader ExecuteDbDataReader(CommandBehavior behavior) => Execute(() => inner.ExecuteReader(behavior));
        protected override void Dispose(bool disposing) { if (disposing) inner.Dispose(); base.Dispose(disposing); }
    }
}
