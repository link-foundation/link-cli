// Physical composition controls prepared before the missing native API exists.
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;
public class LsmStoreBeforeTests
{
    private sealed class Tables : ILsmTableNamespace
    {
        internal readonly SortedDictionary<ulong, PagedTestDevice> Files = new();
        internal readonly HashSet<IPageDevice> Leases = new();
        internal readonly List<string> Mutations = new();
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<ulong> List(int maxEntries)
        { Assert.True(Files.Count <= maxEntries); return Files.Keys.ToArray(); }
        public IPageDevice Create(ulong id)
        {
            Assert.False(Files.ContainsKey(id)); var device = new PagedTestDevice();
            Files.Add(id, device); Assert.True(Leases.Add(device)); Mutations.Add("create"); return device;
        }
        public IPageDevice Open(ulong id)
        { var device = Files[id]; Assert.True(Leases.Add(device)); return device; }
        public void Close(IPageDevice device) { Assert.True(Leases.Remove(device)); Mutations.Add("close"); }
        public void Delete(ulong id)
        { Assert.DoesNotContain(Files[id], Leases); Assert.True(Files.Remove(id)); Mutations.Add("delete"); }
    }
    private static LsmStoreOptions Options() => new(new LsmOptions(32,4096,2,128,2,4,8192),16);
    private static void Put(PhysicalLsmJournalStore store, string key, string[] entries)
    { Assert.True(store.CompareExchange(new[]{key}, store.Read(new[]{key}), new IReadOnlyList<string>?[]{entries})); }
    [Fact]
    public void PhysicalGroupOwnershipTombstoneEmptyAndReopen()
    {
        var root = new PagedTestDevice(); var first = new PagedTestDevice(); var second = new PagedTestDevice();
        var wal = new CheckpointPagedCommitLog(root,first,second,128); var files = new Tables();
        var store = PhysicalLsmJournalStore.Open(wal,files,Options());
        var keys = new[]{"a","b"}; var absent = store.Read(keys);
        Assert.All(absent,v=>Assert.Equal(0UL,v.Generation));
        Assert.True(store.CompareExchange(keys,absent,new IReadOnlyList<string>?[]{new[]{"\ufeff雪\0😀\n"},Array.Empty<string>()}));
        var saved = store.Read(keys); Assert.True(saved[0].Generation>0); Assert.Equal(saved[0].Generation,saved[1].Generation);
        ((string[])saved[0].Entries)[0]="caller"; Assert.Equal("\ufeff雪\0😀\n",store.Read(new[]{"a"})[0].Entries[0]); store.Flush();
        int mutations = files.Mutations.Count;
        Assert.False(store.CompareExchange(keys,absent,new IReadOnlyList<string>?[]{null,new[]{"lost"}}));
        var current = store.Read(keys); Assert.True(store.CompareExchange(keys,current,new IReadOnlyList<string>?[]{current[0].Entries,current[1].Entries}));
        Assert.Equal(mutations,files.Mutations.Count);
        Assert.True(store.CompareDelete("a",current[0].Generation)); var tombstone = store.Read(new[]{"a"})[0];
        Assert.True(tombstone.Generation>current[0].Generation); Assert.Empty(tombstone.Entries);
        Assert.False(store.CompareDelete("a",tombstone.Generation)); Assert.Equal(new[]{"b"},store.ScanNamespace().Select(row=>row.Key));
        Put(store,"a",Array.Empty<string>()); store.Compact(); var expected=store.Read(keys);store.Dispose();Assert.Empty(files.Leases);
        var reopenedWal=new CheckpointPagedCommitLog(root.Crash(),first.Crash(),second.Crash(),128);
        var recovered=new Tables();foreach(var pair in files.Files)recovered.Files.Add(pair.Key,pair.Value.Crash());
        using var reopened=PhysicalLsmJournalStore.Open(reopenedWal,recovered,Options());
        Assert.Equal(expected.Select(v=>v.Generation),reopened.Read(keys).Select(v=>v.Generation));Assert.All(reopened.Read(keys),v=>Assert.Empty(v.Entries));
        Assert.Equal(new[]{"a","b"},reopened.ScanNamespace().Select(row=>row.Key));
    }
    [Fact]
    public void RepeatedBufferAndWalCompactionPreservesFollowingUse()
    {
        var root=new PagedTestDevice();var first=new PagedTestDevice();var second=new PagedTestDevice();
        var wal=new CheckpointPagedCommitLog(root,first,second,128);var files=new Tables();
        using var store=PhysicalLsmJournalStore.Open(wal,files,Options());
        for(int i=0;i<30;i++){Put(store,"k"+(i%5),new[]{i.ToString()});if(i%4==0)store.Flush();}
        store.Compact();Assert.Contains("delete",files.Mutations);Assert.Single(files.Files);
        var state=LsmWal.Recover(wal.Read(),Options().Limits);Assert.Single(state.Manifest);Assert.Empty(state.Buffer);
        Assert.Equal("27",store.Read(new[]{"k2"})[0].Entries[0]);Assert.True(wal.Read().Count<=4);
        Assert.ThrowsAny<Exception>(()=>Put(store,"oversized",new[]{new string('x',129)}));
        Put(store,"following",new[]{"ok"});store.Flush();Assert.Equal("ok",store.Read(new[]{"following"})[0].Entries[0]);
    }
}
