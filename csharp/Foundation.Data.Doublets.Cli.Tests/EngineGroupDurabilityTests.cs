using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineGroupDurabilityTests
{
    private sealed class CountedLog(ICommitLog inner) : ICommitLog
    {
        public int Flushes;
        public bool SupportsDurableFlush => inner.SupportsDurableFlush;
        public IReadOnlyList<string> Read() => inner.Read();
        public void Append(string record) => inner.Append(record);
        public void Flush() { Flushes++; inner.Flush(); }
    }

    [Fact]
    public void DurableDeltaReplicasReopenEveryIndependentRevision()
    {
        var directory = Path.Combine(Path.GetTempPath(), "engine-group-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var paths = new[] { Path.Combine(directory, "primary"), Path.Combine(directory, "follower") };
        var options = new EngineOptions { Isolation = IsolationLevel.Snapshot, HistoryLimit = null, RequireDurableFlush = true };
        try
        {
            using (var primary = new FileCommitLog(paths[0]))
            using (var follower = new FileCommitLog(paths[1]))
            {
                var p = new CountedLog(primary); var f = new CountedLog(follower);
                var engine = new LinksEngine<uint>(new DeltaCommitLog(new ReplicatedCommitLog(p, new[] { f }, 2)), options);
                using (var seed = engine.Begin()) { for (uint id = 1; id <= 32; id++) seed.EnsureCreated(id); seed.Commit(); }
                p.Flushes = f.Flushes = 0;
                using var a = engine.Begin(); a.SetName(1, "one 雪");
                using var b = engine.Begin(); b.SetName(2, "two");
                Assert.Equal(new[] { new GroupCommitOutcome(GroupCommitStatus.Committed, 2), new GroupCommitOutcome(GroupCommitStatus.Committed, 3) }, engine.CommitGroup(new[] { a, b }));
                Assert.Equal(1, p.Flushes); Assert.Equal(1, f.Flushes);
            }
            Assert.Equal(File.ReadAllBytes(paths[0]), File.ReadAllBytes(paths[1]));
            foreach (var path in paths)
            {
                using var raw = new FileCommitLog(path);
                Assert.StartsWith("delta1|", raw.Read()[1]);
                var engine = new LinksEngine<uint>(new DeltaCommitLog(raw), options);
                Assert.Null(engine.SnapshotAt(1).Rows[1].Name);
                Assert.Equal("one 雪", engine.SnapshotAt(2).Rows[1].Name);
                Assert.Null(engine.SnapshotAt(2).Rows[2].Name);
                Assert.Equal("two", engine.SnapshotAt(3).Rows[2].Name);
                using var next = engine.Begin(); Assert.Equal(33u, next.CreateLink(0, 0)); Assert.Equal(4ul, next.Commit());
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private sealed class GateLog : ICommitLog, IDisposable
    {
        private readonly MemoryCommitLog _inner = new();
        public readonly ManualResetEventSlim Entered = new(), Release = new();
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => _inner.Read();
        public void Append(string record) => _inner.Append(record);
        public void Flush()
        {
            Entered.Set();
            if (!Release.Wait(TimeSpan.FromSeconds(5))) throw new IOException("Bounded test gate expired.");
        }
        public void Dispose() { Entered.Dispose(); Release.Dispose(); }
    }

    [Fact]
    public async Task ReadersCannotObserveIntermediatePublicationBeforeFlushReturns()
    {
        using var log = new GateLog();
        var engine = new LinksEngine<uint>(log, new() { Isolation = IsolationLevel.Snapshot });
        using var a = engine.Begin(); a.CreateLink(0, 0);
        using var b = engine.Begin(); b.CreateLink(1, 0);
        using var started = new ManualResetEventSlim();
        var writer = Task.Factory.StartNew(() => engine.CommitGroup(new[] { a, b }), CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
        Task<EngineSnapshot<uint>>? reader = null;
        bool entered = log.Entered.Wait(TimeSpan.FromSeconds(5)), withheld = false;
        try
        {
            if (entered)
            {
                reader = Task.Factory.StartNew(() => { started.Set(); return engine.Snapshot(); }, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
                Assert.True(started.Wait(TimeSpan.FromSeconds(5)));
                withheld = await Task.WhenAny(reader, Task.Delay(25)) != reader;
            }
        }
        finally { log.Release.Set(); }
        var outcomes = await writer.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.True(entered); Assert.True(withheld);
        var snapshot = await reader!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(2ul, snapshot.Revision); Assert.Equal(2, snapshot.Rows.Count);
        Assert.All(outcomes, outcome => Assert.Equal(GroupCommitStatus.Committed, outcome.Status));
    }
}
