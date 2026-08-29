// Embedding Foundation.Data.Doublets.Cli as a transactional store, the
// way an external application would (issue #98).
//
// Run it with:
//   dotnet run --project examples/embedded-store/csharp
//
// It walks through the four properties that make the library reusable:
//
//   1. the transactions layer composes over any unsigned address type —
//      `ulong` here, while the `clink` CLI itself uses `uint`;
//   2. an uncommitted transaction is rolled back when the store reopens;
//   3. the database file is mutated in place, and committed writes
//      survive a crash;
//   4. an advisory lock keeps a second writer out, and StorageRevision
//      tells a reader whether anyone else has written.

using Foundation.Data.Doublets.Cli;
// `CreateAndUpdate` and `Exists` are extension methods on `ILinks<T>`.
using Platform.Data;
using Platform.Data.Doublets;

var directory = args.Length > 0
  ? args[0]
  : Path.Combine(Path.GetTempPath(), "clink-embedded-store");
if (Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
Directory.CreateDirectory(directory);

var database = Path.Combine(directory, "db.links");
var log = Path.Combine(directory, "db.transitions.links");

static Scope Open(string database, string log) => new(database, log);

// -- 1. Commit one write, then abandon another --------------------------
ulong committed;
ulong abandoned;
using (var scope = Open(database, log))
{
    committed = scope.Transactions.CreateAndUpdate(
      scope.Transactions.Constants.Null, scope.Transactions.Constants.Null);
    Console.WriteLine($"committed link {committed}");

    // Beginning a transaction and never committing it is what a crash
    // looks like to the next process that opens the store.
    var abandonedTransaction = scope.Transactions.BeginTransaction();
    abandoned = scope.Transactions.CreateAndUpdate(
      scope.Transactions.Constants.Null, scope.Transactions.Constants.Null);
    Console.WriteLine($"wrote link {abandoned} inside a transaction that never commits");
    GC.KeepAlive(abandonedTransaction);
}

// -- 2. Reopen: recovery runs in the constructor ------------------------
StorageRevision revision;
using (var scope = Open(database, log))
{
    Console.WriteLine(
      $"after recovery: {committed} is present = {scope.Transactions.Exists(committed)}, " +
      $"{abandoned} is present = {scope.Transactions.Exists(abandoned)}");
    revision = StorageRevision.Of(database);
}

// -- 3. Advisory locking ------------------------------------------------
var lockPath = LinksFileLock.LockFilePath(database);
using (LinksFileLock.Acquire(lockPath, LockMode.Exclusive))
{
    var second = LinksFileLock.TryAcquire(lockPath, LockMode.Exclusive);
    Console.WriteLine($"a second writer is locked out: {second is null}");
    second?.Dispose();
}

// -- 4. Notice a write by somebody else ---------------------------------
using (var scope = Open(database, log))
{
    var extra = scope.Transactions.CreateAndUpdate(committed, committed);
    Console.WriteLine($"another holder committed link {extra}");
}

Console.WriteLine($"revision changed: {revision.HasChanged(database)}");

/// <summary>Owns the data store, the sidecar log and the decorator over them.</summary>
internal sealed class Scope : IDisposable
{
    private readonly NamedTypesDecorator<ulong> _data;
    private readonly NamedTypesDecorator<ulong> _log;

    public Scope(string database, string log)
    {
        _data = new NamedTypesDecorator<ulong>(database);
        _log = new NamedTypesDecorator<ulong>(log);
        Transactions = new TransactionsDecorator<ulong>(_data, _log);
    }

    public TransactionsDecorator<ulong> Transactions { get; }

    public void Dispose()
    {
        Transactions.Dispose();
        _data.Dispose();
        _log.Dispose();
    }
}
