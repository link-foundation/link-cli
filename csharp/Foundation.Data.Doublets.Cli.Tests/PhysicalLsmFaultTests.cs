using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class PhysicalLsmFaultTests
{
    private static readonly string[] Keys = { "a", "b", "c", "d" };
    private static void Put(PhysicalLsmJournalStore s, string key, string[] value) => Assert.True(s.CompareExchange(new[] { key }, s.Read(new[] { key }), new IReadOnlyList<string>?[] { value }));
    private static (PhysicalLsmHarness Harness, PhysicalLsmJournalStore Store, IReadOnlyList<JournalVersion> Before) Prepared()
    {
        var h = new PhysicalLsmHarness(); var s = h.Open(); Put(s, "a", new[] { "A" }); Put(s, "b", Array.Empty<string>()); s.Flush();
        Put(s, "a", new[] { "new" }); Assert.True(s.CompareDelete("b", s.Read(new[] { "b" })[0].Generation)); s.Flush(); Put(s, "c", new[] { "C" });
        return (h, s, s.Read(Keys));
    }
    private static bool Change(PhysicalLsmJournalStore s, IReadOnlyList<JournalVersion> before) => s.CompareExchange(new[] { "a", "d" }, new[] { before[0], before[3] }, new IReadOnlyList<string>?[] { new[] { "changed" }, new[] { "D" } });
    private static bool Equal(IReadOnlyList<JournalVersion> left, IReadOnlyList<JournalVersion> right) => left.Count == right.Count && left.Zip(right).All(pair => pair.First.Generation == pair.Second.Generation && pair.First.Entries.SequenceEqual(pair.Second.Entries));
    [Fact(Timeout = 30000)]
    public Task EveryCompactionMutationCutPreservesAcknowledgedValuesAndFollowingUse() => Task.Run(() =>
    {
        var (probe, store, _) = Prepared(); probe.Reset(); store.Compact(); var trace = probe.Trace.ToArray(); store.Dispose();
        Assert.Contains(trace, name => name.StartsWith("create:", StringComparison.Ordinal)); Assert.Contains(trace, name => name.StartsWith("delete:", StringComparison.Ordinal));
        for (int at = 1; at <= trace.Length; at++) foreach (bool lostReply in new[] { false, true })
        {
            var (h, s, before) = Prepared(); h.Reset(); h.Fault = (at, lostReply); Assert.Contains("injected", Assert.Throws<IOException>(s.Compact).Message);
            int calls = h.Trace.Count; Assert.Contains("fenced", Assert.Throws<InvalidOperationException>(() => s.Read(Keys)).Message); Assert.Equal(calls, h.Trace.Count);
            var fresh = new PhysicalLsmHarness(h.Image()); using var reopened = fresh.Open(); Assert.True(Equal(before, reopened.Read(Keys)), at + "/" + lostReply + "/" + trace[at - 1]);
            ulong reserved = LsmWal.Recover(fresh.Wal().Read(), PhysicalLsmHarness.Options().Limits).LastReservedTableId;
            fresh.Reset(); Put(reopened, "following", new[] { "ok" }); reopened.Flush();
            foreach (var name in fresh.Trace.Where(name => name.StartsWith("create:", StringComparison.Ordinal))) Assert.True(ulong.Parse(name[7..], System.Globalization.CultureInfo.InvariantCulture) > reserved);
            reopened.Dispose(); s.Dispose();
        }
        Console.WriteLine($"compaction: {trace.Length} effects; {trace.Length * 2} finite before/after cuts");
    });
    [Fact(Timeout = 30000)]
    public Task EveryGroupWriteMutationCutRecoversWholeOldOrNewGroup() => Task.Run(() =>
    {
        var (probe, store, beforeProbe) = Prepared(); probe.Reset(); Assert.True(Change(store, beforeProbe)); var trace = probe.Trace.ToArray(); var after = store.Read(Keys); store.Dispose();
        Assert.Contains(trace, name => name.StartsWith("create:", StringComparison.Ordinal)); Assert.Contains(trace, name => name.StartsWith("delete:", StringComparison.Ordinal));
        for (int at = 1; at <= trace.Length; at++) foreach (bool lostReply in new[] { false, true })
        {
            var (h, s, before) = Prepared(); h.Reset(); h.Fault = (at, lostReply); Assert.Contains("injected", Assert.Throws<IOException>(() => Change(s, before)).Message);
            int calls = h.Trace.Count; Assert.Throws<InvalidOperationException>(() => s.Read(Keys)); Assert.Equal(calls, h.Trace.Count);
            var fresh = new PhysicalLsmHarness(h.Image()); using var reopened = fresh.Open(); var actual = reopened.Read(Keys);
            Assert.True(Equal(actual, before) || Equal(actual, after), at + "/" + lostReply + "/" + trace[at - 1]);
            Put(reopened, "following", new[] { "ok" }); reopened.Compact(); var image = fresh.Image(); reopened.Dispose(); s.Dispose();
            var next = new PhysicalLsmHarness(image); using var following = next.Open(); Assert.Equal("ok", following.Read(new[] { "following" })[0].Entries[0]);
        }
        Console.WriteLine($"group: {trace.Length} effects; {trace.Length * 2} finite before/after cuts");
    });
}
