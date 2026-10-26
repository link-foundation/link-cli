using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

// Isolated finite probes run beside the ordinary tests. They never change the
// testhost's global ThreadPool limits or terminate the parent test process.
internal static class ProbeProgram
{
    private static int Main(string[] args)
    {
        try
        {
            if (args is ["async-worker-pool"]) return AsyncWorkerPool();
            if (args is ["deep-structure", var structureDirectory]) return StructureDepthTests.Child(structureDirectory);
            if (args is ["unicode-cycle", var unicodeOperation, var unicodeDirectory]) return UnicodeGraphCycleTests.Child(unicodeOperation, unicodeDirectory);
            if (args is ["group-exit", var groupPath, var groupPoint, var delta]) return EngineGroupExitTests.Child(groupPath, groupPoint, bool.Parse(delta));
            if (args is ["group-exit", var encodedPath, var encodedPoint, var physicalDelta, var encoding, var structure])
                return EngineGroupExitTests.Child(encodedPath, encodedPoint, bool.Parse(physicalDelta),
                    Enum.Parse<Engine.RevisionEncoding>(encoding), Enum.Parse<Engine.StateStructure>(structure));
            if (args is ["pages-exit", var pagesPath, var pagesPoint]) return PagedFileExitTests.Child(pagesPath, pagesPoint);
            if (args is ["sqlite-exit", var sqlitePath, var sqlitePoint, var sqliteDelta, var sqliteEncoding, var sqliteStructure])
                return EngineSqliteExitTests.Child(sqlitePath, sqlitePoint, bool.Parse(sqliteDelta),
                    Enum.Parse<Engine.RevisionEncoding>(sqliteEncoding), Enum.Parse<Engine.StateStructure>(sqliteStructure));
            if (args is ["row-wal-exit", var rowDirectory, var rowOperation, var rowExplicit])
                return TransactionRowWalExitTests.Child(rowDirectory, rowOperation, bool.Parse(rowExplicit));
            if (args is ["deep-ancestry", var operation, var ancestryDirectory]) return VersionAncestryDepthTests.Child(operation, ancestryDirectory);
            if (args is ["version-exit", var directory, var mode, var automatic])
                return VersionExitProbe.Run(directory, mode, bool.Parse(automatic));
            if (args is ["name-exit", var nameDirectory, var nameMode, var nameAutomatic])
                return VersionExitProbe.Run(nameDirectory, nameMode, bool.Parse(nameAutomatic), names: true);
            if (args is ["movement-exit", var movementDirectory, var point, var branchSwitch])
                return VersionMovementExitTests.Child(movementDirectory, point, bool.Parse(branchSwitch));
            if (args is ["journal-quota", var journalDirectory, var journalMode, var unicode])
                return JournalLiveTailTests.QuotaChild(journalDirectory, journalMode, bool.Parse(unicode));
            Console.Error.WriteLine("Unknown finite test probe.");
            return 2;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error);
            return 1;
        }
    }

    private static int AsyncWorkerPool()
    {
        ThreadPool.GetMinThreads(out _, out var completionMin);
        ThreadPool.GetMaxThreads(out _, out var completionMax);
        if (!ThreadPool.SetMinThreads(1, completionMin) || !ThreadPool.SetMaxThreads(2, completionMax))
            throw new InvalidOperationException("Cannot configure isolated two-worker pool.");
        var directory = Directory.CreateTempSubdirectory("bounded-worker-pool-").FullName;
        var stores = new List<NamedTypesDecorator<uint>>();
        var owners = new List<TransactionsDecorator>();
        using var canary = new ManualResetEventSlim();
        try
        {
            for (var index = 0; index < 4; ++index)
            {
                var data = new NamedTypesDecorator<uint>(Path.Combine(directory, $"data{index}.links"));
                stores.Add(data);
                var log = new NamedTypesDecorator<uint>(Path.Combine(directory, $"log{index}.links"));
                stores.Add(log);
                owners.Add(new TransactionsDecorator(data, log, commitMode: index == 3 ? CommitMode.Async : CommitMode.Sync));
            }
            var active = owners[^1];
            active.CreateAndUpdate(11u, 12u);
            ThreadPool.QueueUserWorkItem(_ => canary.Set());
            var responsive = canary.Wait(TimeSpan.FromSeconds(3));
            var applied = responsive && SpinWait.SpinUntil(() => active.AppliedSequence == active.LastLoggedSequence, TimeSpan.FromSeconds(3));
            Console.WriteLine($"Two-worker pool: responsive={responsive}, checkpoint={applied}");
            return responsive && applied ? 0 : 1;
        }
        finally
        {
            // Creation order releases the earliest blocking baseline workers.
            foreach (var owner in owners) owner.Shutdown();
            foreach (var store in stores.AsEnumerable().Reverse()) store.Dispose();
            Directory.Delete(directory, recursive: true);
        }
    }
}
