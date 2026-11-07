using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineReadSetTests
{
    private static LinksEngine<ulong> Engine()
    {
        var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() { SerializableValidation = SerializableValidation.ReadSet });
        using var seed = engine.Begin(); seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit();
        return engine;
    }
    [Fact]
    public void OptionsRequireSerializableAndExclusiveStillFencesWriters()
    {
        foreach (var isolation in new[] { IsolationLevel.ReadCommitted, IsolationLevel.Snapshot })
            Assert.Throws<ArgumentException>(() => new LinksEngine<ulong>(new MemoryCommitLog(),
                new() { Isolation = isolation, SerializableValidation = SerializableValidation.ReadSet }));
        Assert.Throws<ArgumentException>(() => new LinksEngine<ulong>(new MemoryCommitLog(),
            new() { SerializableValidation = (SerializableValidation)999 }));
        var exclusive = new LinksEngine<ulong>(new MemoryCommitLog(),
            new() { Concurrency = ConcurrencyControl.Exclusive, SerializableValidation = SerializableValidation.ReadSet });
        using var writer = exclusive.Begin(); writer.CreateLink(0, 0);
        Assert.Throws<InvalidOperationException>(() => exclusive.Begin());
        writer.Commit(); using var next = exclusive.Begin();
        Assert.Single(exclusive.Snapshot().Rows);
    }
    [Fact]
    public void DisjointPointEditsCommitButReadDependenciesPreventWriteSkew()
    {
        var engine = Engine();
        using var a = engine.Begin(); using var b = engine.Begin();
        Assert.Null(a.GetName(1)); Assert.Null(b.GetName(2));
        a.SetName(1, "one"); b.SetName(2, "two");
        Assert.Equal(2ul, a.Commit()); Assert.Equal(3ul, b.Commit());
        using var c = engine.Begin(); using var d = engine.Begin();
        Assert.Equal("two", c.GetName(2)); Assert.Equal("one", d.GetName(1));
        c.RemoveName(1); d.RemoveName(2); c.Commit();
        Assert.Contains("read", Assert.Throws<InvalidOperationException>(() => d.Commit()).Message, StringComparison.OrdinalIgnoreCase);
        Assert.Equal("two", engine.Snapshot().Rows[2].Name);
    }
    [Fact]
    public void MissingReadsAbaAndBranchesInvalidateStaleReaders()
    {
        var engine = Engine();
        using var missing = engine.Begin(); Assert.Null(missing.GetLink(99)); missing.SetName(2, "abort");
        using (var insert = engine.Begin()) { insert.EnsureCreated(99); insert.Commit(); }
        Assert.Throws<InvalidOperationException>(() => missing.Commit());
        using var aba = engine.Begin(); Assert.Null(aba.GetName(1));
        using (var deleted = engine.Begin()) { deleted.DeleteLink(1); deleted.Commit(); }
        using (var recreated = engine.Begin()) { recreated.EnsureCreated(1); recreated.Commit(); }
        Assert.Throws<InvalidOperationException>(() => aba.Commit());
        var branch = engine.Fork(new MemoryCommitLog());
        using var stale = engine.Begin(); Assert.Null(stale.GetName(1));
        using (var incoming = branch.Engine.Begin()) { incoming.SetName(1, "branch"); incoming.Commit(); }
        branch.Merge(); Assert.Throws<InvalidOperationException>(() => stale.Commit());
        var log = engine.WithLog(log =>
        {
            var copy = new MemoryCommitLog();
            foreach (var record in log.Read()) copy.Append(record);
            return copy;
        });
        var reopened = new LinksEngine<ulong>(log, new() { SerializableValidation = SerializableValidation.ReadSet });
        Assert.Equal("branch", reopened.Snapshot().Rows[1].Name);
    }
    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public void ScansAndNameSearchesInvalidateOnAnyRevision(int scan)
    {
        var engine = Engine();
        using var reader = engine.Begin();
        if (scan == 0) Assert.Equal(2ul, reader.Count(null));
        else if (scan == 1) Assert.Equal(0ul, reader.GetByName("absent"));
        else reader.Each(new ulong[] { 1 }, null);
        using (var writer = engine.Begin()) { writer.SetName(2, "changed"); writer.Commit(); }
        Assert.Throws<InvalidOperationException>(() => reader.Commit());
    }
    [Fact]
    public void ReadOnlyCommitsCheckOnlyObservedDependencies()
    {
        var engine = Engine();
        using var independent = engine.Begin(); Assert.Null(independent.GetName(1));
        using (var writer = engine.Begin()) { writer.SetName(2, "changed"); writer.Commit(); }
        Assert.Equal(engine.Snapshot().Revision, independent.Commit());
        using var empty = engine.Begin();
        using (var writer = engine.Begin()) { writer.SetName(1, "after"); writer.Commit(); }
        empty.Commit();
        using var reader = engine.Begin(); Assert.Equal("after", reader.GetName(1));
        using (var writer = engine.Begin()) { writer.RemoveName(1); writer.Commit(); }
        Assert.Throws<InvalidOperationException>(() => reader.Commit());
    }
    [Fact]
    public void OwnWritesAndBlindAllocationsValidateWrittenAddresses()
    {
        var engine = Engine();
        using var own = engine.Begin(); var id = own.CreateLink(1, 2);
        Assert.Equal(1ul, own.GetLink(id)!.Value.Source);
        own.SetName(id, "own"); Assert.Equal("own", own.GetName(id));
        Assert.Equal(3ul, own.Count(null)); own.DeleteLink(id); Assert.Equal(2ul, own.Count(null)); own.Commit();
        using var allocated = engine.Begin(); var reserved = allocated.CreateLink(0, 0);
        using (var assigned = engine.Begin())
        { assigned.EnsureCreated(reserved); assigned.SetName(reserved, "winner"); assigned.Commit(); }
        Assert.Contains("written", Assert.Throws<InvalidOperationException>(() => allocated.Commit()).Message, StringComparison.OrdinalIgnoreCase);
        Assert.Equal("winner", engine.Snapshot().Rows[reserved].Name);
        using var retry = engine.Begin(); retry.SetName(reserved, "fresh"); retry.Commit();
    }
    // Enumerate serial histories independently of the generation implementation.
    // 216 profiles x six commit orders; three sessions, two rows per probe.
    [Fact(Timeout = 30000)]
    public async Task FiniteConcurrentHistoriesHaveASerialExecution()
    {
        await Task.Yield();
        int[][] orders = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        for (int profile = 0; profile < 216; profile++)
        {
            int[] choices = [profile % 6, profile / 6 % 6, profile / 36];
            var reads = choices.Select(c => c / 2 + 1).ToArray();
            var writes = choices.Select(c => c % 2 + 1).ToArray();
            foreach (var order in orders)
            {
                var engine = Engine();
                var sessions = Enumerable.Range(0, 3).Select(_ => engine.Begin()).ToArray();
                try
                {
                    for (int i = 0; i < 3; i++)
                    {
                        Assert.Null(sessions[i].GetName((ulong)reads[i]));
                        Assert.Null(sessions[i].GetName((ulong)writes[i]));
                        sessions[i].SetName((ulong)writes[i], i.ToString(System.Globalization.CultureInfo.InvariantCulture));
                    }
                    bool[] accepted = new bool[3];
                    foreach (int i in order)
                    {
                        try { sessions[i].Commit(); accepted[i] = true; }
                        catch (InvalidOperationException error) { Assert.Contains("conflict", error.Message, StringComparison.OrdinalIgnoreCase); }
                    }
                    var actual = engine.Snapshot();
                    Assert.True(orders.Any(serial =>
                    {
                        string?[] names = [null, null];
                        foreach (int i in serial)
                        {
                            if (!accepted[i]) continue;
                            if ((reads[i] <= 2 && names[reads[i] - 1] is not null) || names[writes[i] - 1] is not null) return false;
                            names[writes[i] - 1] = i.ToString(System.Globalization.CultureInfo.InvariantCulture);
                        }
                        return names[0] == actual.Rows[1].Name && names[1] == actual.Rows[2].Name;
                    }), $"Nonserial history profile={profile} order={string.Join(',', order)}");
                }
                finally { foreach (var session in sessions) session.Dispose(); }
            }
        }
    }
}
