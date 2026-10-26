using System.Buffers.Binary;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

// Finite durable-image device: writes and their individual persistence barriers
// are separate. A complete unsynced decision may reach storage before a reply.
internal sealed class PagedTestDevice : IRepairPageDevice
{
    internal sealed class Image
    {
        internal byte[]?[] Metadata = new byte[]?[2];
        internal List<byte[]> Pages = new();
        internal Image Copy() => new() { Metadata = Metadata.Select(p => p?.ToArray()).ToArray(), Pages = Pages.Select(p => p.ToArray()).ToList() };
    }
    internal Image Live = new(), Durable = new();
    internal readonly List<string> Calls = new();
    internal (int At, bool After)? Fault;
    internal bool ReadError;
    internal int Reads;
    internal int Completed;
    public uint MetadataFlags { get; set; }
    public void CompleteRecovery() { Completed++; }
    public bool SupportsDurableFlush { get; set; } = true;
    public byte[] NewDatabaseId() { Reads++; return Enumerable.Range(0, 16).Select(i => (byte)i).ToArray(); }
    public long DataLength { get { Reads++; return Live.Pages.Count * 4096L; } }
    public byte[]? ReadMetadata(int slot) { Reads++; return Live.Metadata[slot]?.ToArray(); }
    public byte[] ReadPage(ulong index)
    {
        Calls.Add("readPage"); if (ReadError) throw new IOException("injected read"); return Live.Pages[(int)index].ToArray();
    }
    private void Action(string name, Action body)
    {
        Calls.Add(name); bool fault = Fault?.At == Calls.Count;
        if (fault && !Fault!.Value.After) throw new IOException("injected " + name);
        body();
        if (fault)
        {
            if (name.StartsWith("writeMetadata", StringComparison.Ordinal)) Durable.Metadata = Live.Metadata.Select(p => p?.ToArray()).ToArray();
            throw new IOException("injected " + name);
        }
    }
    public void WritePage(ulong index, ReadOnlySpan<byte> bytes)
    {
        byte[] copy = bytes.ToArray(); Action("writePage", () => { Assert.Equal((ulong)Live.Pages.Count, index); Live.Pages.Add(copy); });
    }
    public void SyncData() => Action("syncData", () => Durable.Pages = Live.Pages.Select(p => p.ToArray()).ToList());
    public void RepairPage(ulong index, ReadOnlySpan<byte> bytes)
    {
        byte[] copy = bytes.ToArray(); Action("repairPage", () => { if (index == (ulong)Live.Pages.Count) Live.Pages.Add(copy); else Live.Pages[(int)index] = copy; });
    }
    public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes)
    {
        byte[] copy = bytes.ToArray(); Action("writeMetadata" + slot, () => Live.Metadata[slot] = copy);
    }
    public void SyncMetadata(int slot) => Action("syncMetadata" + slot, () => Durable.Metadata[slot] = Live.Metadata[slot]?.ToArray());
    public void TruncateData(long length) => Action("truncateData", () => Live.Pages.RemoveRange((int)(length / 4096), Live.Pages.Count - (int)(length / 4096)));
    internal PagedTestDevice Crash() => From(Durable);
    internal static PagedTestDevice From(Image image) => new() { Live = image.Copy(), Durable = image.Copy() };
    internal static (Image Image, string[] Records) Fixture(string name)
    {
        using var json = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/native-pages-v1.json")));
        var fixture = json.RootElement.GetProperty(name);
        return (new Image
        {
            Metadata = fixture.GetProperty("metadata").EnumerateArray().Select(p => p.GetString() is "" ? null : Convert.FromHexString(p.GetString()!)).ToArray(),
            Pages = fixture.GetProperty("pages").EnumerateArray().Select(p => Convert.FromHexString(p.GetString()!)).ToList()
        }, fixture.GetProperty("records").EnumerateArray().Select(p => p.GetString()!).ToArray());
    }
    internal static void Same(Image expected, Image actual)
    {
        Assert.Equal(expected.Metadata[0], actual.Metadata[0]); Assert.Equal(expected.Metadata[1], actual.Metadata[1]);
        Assert.Equal(expected.Pages.Count, actual.Pages.Count);
        for (int i = 0; i < expected.Pages.Count; i++) Assert.Equal(expected.Pages[i], actual.Pages[i]);
    }
    internal static void Set(byte[] bytes, int offset, ulong value) => BinaryPrimitives.WriteUInt64LittleEndian(bytes.AsSpan(offset), value);
    private static ulong Hash(IEnumerable<byte> bytes) => bytes.Aggregate(14695981039346656037UL, (sum, b) => unchecked((sum ^ b) * 1099511628211));
    internal static void Sign(byte[] metadata) => Set(metadata, 120, Hash(metadata.Take(120)));
    internal static void SignPages(Image image)
    {
        ulong previous = 0;
        foreach (var page in image.Pages) { Set(page, 48, previous); previous = Hash(page.Take(56).Concat(page.Skip(64))); Set(page, 56, previous); }
        byte[] metadata = image.Metadata[image.Pages.Count == 1 ? 1 : 0]!; Set(metadata, 72, previous); Sign(metadata);
    }
}
