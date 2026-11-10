using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;
using static Foundation.Data.Doublets.Cli.Tests.PagedTestDevice;
namespace Foundation.Data.Doublets.Cli.Tests;

internal sealed class MirroredTestDevice : IRepairPageDevice
{
    internal sealed class Trace { internal List<string> Calls = new(), Reads = new(); internal (int At, bool After)? Fault; }
    internal Image Live, Durable;
    internal readonly Trace Mutations;
    internal readonly int Side;
    internal bool ReadError, MetadataError;
    internal long? Length;
    internal Action<ulong>? BeforeRead;
    public bool SupportsDurableFlush { get; set; } = true;
    internal MirroredTestDevice(Image image, Trace trace, int side) { Live = image.Copy(); Durable = image.Copy(); Mutations = trace; Side = side; }
    public long DataLength { get { Mutations.Reads.Add(Side + ":length"); return Length ?? Live.Pages.Count * 4096L; } }
    public byte[] NewDatabaseId() => Enumerable.Range(0, 16).Select(i => (byte)i).ToArray();
    public byte[]? ReadMetadata(int slot) { Mutations.Reads.Add(Side + ":metadata"); return MetadataError ? throw new IOException("injected metadata read") : Live.Metadata[slot]?.ToArray(); }
    public byte[] ReadPage(ulong index)
    {
        Mutations.Reads.Add(Side + ":page"); BeforeRead?.Invoke(index);
        if (ReadError || index >= (ulong)Live.Pages.Count) throw new IOException("injected short page");
        return Live.Pages[(int)index].ToArray();
    }
    private void Action(string name, Action effect)
    {
        Mutations.Calls.Add(Side + ":" + name); bool fault = Mutations.Fault?.At == Mutations.Calls.Count;
        if (fault && !Mutations.Fault!.Value.After) throw new IOException("injected before " + name);
        effect();
        if (fault) { if (name.StartsWith("writeMetadata", StringComparison.Ordinal)) Durable.Metadata = Live.Metadata.Select(b => b?.ToArray()).ToArray(); throw new IOException("injected after " + name); }
    }
    public void WritePage(ulong index, ReadOnlySpan<byte> bytes)
    { var copy = bytes.ToArray(); Action("writePage", () => { Assert.Equal((ulong)Live.Pages.Count, index); Live.Pages.Add(copy); }); }
    public void RepairPage(ulong index, ReadOnlySpan<byte> bytes)
    { var copy = bytes.ToArray(); Action("repairPage", () => { Assert.True(index <= (ulong)Live.Pages.Count); if (index == (ulong)Live.Pages.Count) Live.Pages.Add(copy); else Live.Pages[(int)index] = copy; }); }
    public void SyncData() => Action("syncData", () => Durable.Pages = Live.Pages.Select(b => b.ToArray()).ToList());
    public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes) { var copy = bytes.ToArray(); Action("writeMetadata" + slot, () => Live.Metadata[slot] = copy); }
    public void SyncMetadata(int slot) => Action("syncMetadata" + slot, () => Durable.Metadata[slot] = Live.Metadata[slot]?.ToArray());
    public void TruncateData(long length) => Action("truncateData", () => Live.Pages.RemoveRange((int)(length / 4096), Live.Pages.Count - (int)(length / 4096)));
    internal sealed class Pair
    {
        internal readonly MirroredTestDevice[] Devices;
        internal readonly Trace Mutations = new();
        internal Pair(Image first, Image second) { Devices = new[] { new MirroredTestDevice(first, Mutations, 0), new MirroredTestDevice(second, Mutations, 1) }; }
        internal PagedCommitLog Open() => new(new MirroredPageDevice(Devices[0], Devices[1]));
        internal Pair Crash() => new(Devices[0].Durable, Devices[1].Durable);
        internal void Following(IEnumerable<string> expected)
        { var log = Open(); Assert.Equal(expected, log.Read()); log.Append("following"); log.Flush(); Assert.Equal(expected.Append("following"), Crash().Open().Read()); }
    }
    internal static JsonDocument Wire() => JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/native-pages-mirrored-v1.json")));
    internal static (Image Image, string[] Records) MirroredFixture(string name)
    {
        using var json = Wire(); var v = json.RootElement.GetProperty(name);
        return (new Image { Metadata = v.GetProperty("metadata").EnumerateArray().Select(p => p.GetString() is "" ? null : Convert.FromHexString(p.GetString()!)).ToArray(), Pages = v.GetProperty("pages").EnumerateArray().Select(p => Convert.FromHexString(p.GetString()!)).ToList() }, v.GetProperty("records").EnumerateArray().Select(p => p.GetString()!).ToArray());
    }
}
