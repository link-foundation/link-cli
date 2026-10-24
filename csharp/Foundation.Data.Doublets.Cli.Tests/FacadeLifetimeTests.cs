using System.Reflection;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class FacadeLifetimeTests
{
    public class Resource : DispatchProxy, IDisposable
    {
        public ILinks<uint>? Inner;
        public ILinks<uint>? Other;
        public Action? OnDispose;
        public int DisposeCalls;
        public void Dispose() { Dispose(true); GC.SuppressFinalize(this); }
        protected virtual void Dispose(bool disposing)
        { if (disposing) { DisposeCalls++; OnDispose?.Invoke(); } }
        protected override object? Invoke(MethodInfo? method, object?[]? arguments) =>
            throw new NotSupportedException();
    }
    private static ILinks<uint> Proxy(out Resource resource)
    {
        var facade = DispatchProxy.Create<ILinks<uint>, Resource>();
        resource = (Resource)(object)facade;
        return facade;
    }

    [Fact]
    public void BorrowerDisposesBeforeItsDependencyAndNestedTraversalSharesVisitedSet()
    {
        var inner = Proxy(out var dependency);
        var outer = Proxy(out var borrower); borrower.Inner = inner;
        borrower.OnDispose = () =>
        {
            Assert.Equal(0, dependency.DisposeCalls);
            LinksFacadeDisposer.Dispose(inner);
        };
        LinksFacadeDisposer.Dispose(outer);
        Assert.Equal(1, borrower.DisposeCalls); Assert.Equal(1, dependency.DisposeCalls);
    }

    [Fact]
    public void SharedAndCyclicFacadeReferencesDisposeOncePerTraversal()
    {
        var first = Proxy(out var a); var second = Proxy(out var b);
        a.Inner = second; a.Other = second; b.Inner = first;
        LinksFacadeDisposer.Dispose(first);
        Assert.Equal(1, a.DisposeCalls); Assert.Equal(1, b.DisposeCalls);
    }

    [Fact]
    public void CleanupMayDetachItsInnerReferencesWithoutLeakingDependencies()
    {
        var inner = Proxy(out var dependency); var outer = Proxy(out var borrower);
        borrower.Inner = inner; borrower.OnDispose = () => borrower.Inner = null;
        LinksFacadeDisposer.Dispose(outer);
        Assert.Null(borrower.Inner); Assert.Equal(1, dependency.DisposeCalls);
    }

    [Fact]
    public void FailedBorrowerCleanupPreservesDependenciesForRetry()
    {
        var inner = Proxy(out var dependency); var outer = Proxy(out var borrower);
        borrower.Inner = inner; var fail = true;
        borrower.OnDispose = () => { if (fail) throw new IOException("Injected cleanup failure"); };
        Assert.Throws<IOException>(() => LinksFacadeDisposer.Dispose(outer));
        Assert.Equal(0, dependency.DisposeCalls); fail = false;
        LinksFacadeDisposer.Dispose(outer);
        Assert.Equal(2, borrower.DisposeCalls); Assert.Equal(1, dependency.DisposeCalls);
    }

    private sealed class TrackedStore(string path, ManualResetEventSlim disposed) : NamedTypesDecorator<uint>(path)
    {
        public void CheckBorrowedLifetime()
        {
            ObjectDisposedException.ThrowIf(disposed.IsSet, this);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) disposed.Set();
            base.Dispose(disposing);
        }
    }

    [Fact]
    public void VersionFacadeRollsBackItsOpenTransactionBeforeClosingStores()
    {
        var directory = Directory.CreateTempSubdirectory("facade-version-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            using var data = new NamedTypesDecorator<uint>(dataPath);
            using var log = new NamedTypesDecorator<uint>(logPath);
            using var branches = new NamedTypesDecorator<uint>(branchPath);
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            using var pending = vc.BeginTransaction();
            var created = vc.CreateAndUpdate(0u, 0u);
            Assert.True(data.Exists(created));
            LinksFacadeDisposer.Dispose(vc);
            Assert.True(pending.IsRolledBack);
            foreach (var path in Directory.EnumerateFiles(directory))
            {
                using var exclusive = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
                Assert.True(exclusive.CanWrite);
            }
            using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
            using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
            using var recovered = new TransactionsDecorator(recoveredData, recoveredLog);
            Assert.False(recovered.Exists(created));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact(Timeout = 20000)]
    public async Task FacadeGraphJoinsCheckpointBeforeClosingMappedDependencies()
    {
        var directory = Directory.CreateTempSubdirectory("facade-lifetime-").FullName;
        using var entered = new ManualResetEventSlim(); using var release = new ManualResetEventSlim();
        using var started = new ManualResetEventSlim(); using var closed = new ManualResetEventSlim();
        var armed = false; Exception? callbackError = null; Task? disposing = null;
        try
        {
            using var data = new TrackedStore(Path.Combine(directory, "data.links"), closed);
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            void Flush()
            {
                if (Volatile.Read(ref armed))
                {
                    entered.Set();
                    if (!release.Wait(TimeSpan.FromSeconds(12))) throw new IOException("Bounded flush gate expired.");
                    // This probe intentionally aborts the checkpoint. Check only
                    // managed ownership after the gate: real filesystem flushes
                    // add unrelated platform latency to the bounded worker join.
                    try { data.CheckBorrowedLifetime(); }
                    catch (Exception error) { callbackError = error; throw; }
                    throw new IOException("Probe skips native checkpoint markers.");
                }
                data.Flush();
            }
            using var tx = new TransactionsDecorator(data, log, commitMode: CommitMode.Async,
                flushData: Flush, flushLog: log.Flush);
            var worker = (Task)typeof(TransactionsDecorator<uint>)
                .GetField("_backgroundWorker", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(tx)!;
            try
            {
                Volatile.Write(ref armed, true);
                using (var transaction = tx.BeginTransaction())
                { tx.CreateAndUpdate(0u, 0u); transaction.Commit(); }
                Assert.True(entered.Wait(TimeSpan.FromSeconds(5)));
                disposing = Task.Factory.StartNew(() =>
                {
                    started.Set(); LinksFacadeDisposer.Dispose(tx);
                }, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
                Assert.True(started.Wait(TimeSpan.FromSeconds(3)));
                Assert.False(closed.Wait(TimeSpan.FromMilliseconds(500)),
                    "The facade traversal closed a borrowed store while its checkpoint still used it.");
                Assert.False(disposing.IsCompleted);
            }
            finally
            {
                release.Set();
                await worker.WaitAsync(TimeSpan.FromSeconds(5));
                if (disposing is not null) await disposing.WaitAsync(TimeSpan.FromSeconds(5));
            }
            Assert.Null(callbackError); Assert.True(closed.IsSet);
            Assert.True(tx.LastLoggedSequence > 0); Assert.Equal(0, tx.AppliedSequence);
            LinksFacadeDisposer.Dispose(tx);
        }
        finally { release.Set(); Directory.Delete(directory, recursive: true); }
    }
}
