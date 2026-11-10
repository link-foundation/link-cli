using static Foundation.Data.Doublets.Cli.Engine.PageFormat;
namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Optional two-copy repair over caller-owned exclusive devices.
/// Both data syncs precede either metadata write; the primary metadata copy is
/// durable before the secondary is written. Repair follows full logical
/// prefix validation. Mutation failure fences this owner until reopen.</summary>
public class MirroredPageDevice : IPageDevice
{
    private readonly IRepairPageDevice[] _devices;
    private readonly SortedDictionary<ulong, Repair> _pages = new();
    private readonly Dictionary<int, MetaRepair> _metadata = new();
    private bool _fenced;
    private sealed record Repair(ulong Expected, int Source, int Destination);
    private sealed record MetaRepair(byte[] Bytes, int[] Sides);
    public MirroredPageDevice(IRepairPageDevice primary, IRepairPageDevice secondary)
    {
        ArgumentNullException.ThrowIfNull(primary); ArgumentNullException.ThrowIfNull(secondary);
        if (ReferenceEquals(primary, secondary)) throw new ArgumentException("Mirroring requires two distinct exclusive devices.");
        _devices = new[] { primary, secondary };
    }
    public uint MetadataFlags => 1;
    public bool SupportsDurableFlush => _devices.All(d => d.SupportsDurableFlush);
    protected void Fence() => _fenced = true;
    private void Check() { if (_fenced) throw Invalid("uncertain mirror owner is fenced; reopen both devices"); }
    private void Mutate(Action action) { Check(); try { action(); } catch { Fence(); throw; } }
    private void Both(Action<IRepairPageDevice> action) => Mutate(() => { foreach (var device in _devices) action(device); });
    private delegate void Write(IRepairPageDevice device, ReadOnlySpan<byte> bytes);
    private void BothWrite(ReadOnlySpan<byte> bytes, Write write)
    { Check(); try { foreach (var device in _devices) write(device, bytes); } catch { Fence(); throw; } }
    public byte[] NewDatabaseId() { Check(); return _devices[0].NewDatabaseId(); }
    public long DataLength
    {
        get
        {
            Check(); long length = 0;
            foreach (var device in _devices) { long n = device.DataLength; if (n < 0 || (ulong)n > MaxPages * PageSize) throw Invalid("mirror physical length"); length = Math.Max(length, n); }
            return length;
        }
    }
    public byte[]? ReadMetadata(int slot)
    {
        Check(); if (slot is not (0 or 1)) throw Invalid("mirror metadata slot");
        bool failed = false;
        var raw = _devices.Select(d => { try { return d.ReadMetadata(slot); } catch (IOException) { failed = true; return new byte[1]; } }).ToArray();
        var states = raw.Select(bytes => Metadata.Decode(bytes, 1)).ToArray();
        if (states[0] is { } a && states[1] is { } b && (!a.Identity.AsSpan().SequenceEqual(b.Identity) || (a.Generation == b.Generation && !raw[0]!.AsSpan().SequenceEqual(raw[1]!)))) throw Invalid("conflicting mirror metadata copies");
        if (states[0] is null && states[1] is null)
        {
            bool[] absent = raw.Select(bytes => bytes is not { Length: > 0 }).ToArray();
            if (absent[0] && absent[1]) return null;
            // The secondary copy is written only after the primary copy is
            // durable. A torn primary next to a never written secondary is a
            // decision interrupted before its acknowledgement: report the
            // invalid bytes like one device so the other slot is used.
            // Garbage in both copies needs a second fault and is refused.
            if (!failed && absent[1]) { _metadata.Remove(slot); return raw[0]; }
            throw Invalid("both mirror metadata copies are unreadable");
        }
        int side = states[0] is null || (states[1] is not null && states[1]!.Generation >= states[0]!.Generation) ? 1 : 0;
        byte[] bytes = raw[side]!.ToArray();
        _metadata[slot] = new(bytes, Enumerable.Range(0, 2).Where(i => raw[i] is null || !raw[i]!.AsSpan().SequenceEqual(bytes)).ToArray());
        return bytes.ToArray();
    }
    private static bool Checked(byte[]? page, ulong expected) => page is { Length: PageSize } && PageHash(page) == expected && U64(page, 56) == expected;
    public byte[] ReadPage(ulong index) => throw Invalid("expected page checksum is required for mirror");
    public byte[] ReadCheckedPage(ulong index, ulong expected)
    {
        Check(); var copies = _devices.Select(d => { try { return d.ReadPage(index); } catch (IOException) { return null; } }).ToArray();
        bool[] valid = copies.Select(p => Checked(p, expected)).ToArray();
        if (!valid.Any(v => v)) throw Invalid("neither mirror page copy matches expected checksum");
        if (valid.All(v => v) && !copies[0]!.AsSpan().SequenceEqual(copies[1]!)) throw Invalid("conflicting mirror page copies");
        int source = valid[0] ? 0 : 1;
        if (!valid.All(v => v)) _pages[index] = new(expected, source, 1 - source);
        return copies[source]!;
    }
    public void CompleteRecovery() => Mutate(() =>
    {
        foreach (var (index, repair) in _pages)
        {
            var page = _devices[repair.Source].ReadPage(index);
            if (!Checked(page, repair.Expected)) throw Invalid("mirror repair source changed");
            _devices[repair.Destination].RepairPage(index, page);
        }
        if (_pages.Count > 0) foreach (var device in _devices) device.SyncData();
        foreach (var (slot, repair) in _metadata)
        {
            foreach (int side in repair.Sides)
            { _devices[side].WriteMetadata(slot, repair.Bytes); _devices[side].SyncMetadata(slot); }
        }
        _pages.Clear(); _metadata.Clear();
    });
    public void WritePage(ulong index, ReadOnlySpan<byte> bytes) => BothWrite(bytes, (d, b) => d.WritePage(index, b));
    public void SyncData() => Both(d => d.SyncData());
    /// <summary>The secondary copy is written only after the primary copy is
    /// durable, so power loss tears at most one copy of a decision.</summary>
    public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes)
    {
        Check();
        try { _devices[0].WriteMetadata(slot, bytes); _devices[0].SyncMetadata(slot); _devices[1].WriteMetadata(slot, bytes); }
        catch { Fence(); throw; }
    }
    /// <summary>Only the secondary copy can be unsynced: writes and repairs
    /// sync the primary copy before anything else is written.</summary>
    public void SyncMetadata(int slot) => Mutate(() => _devices[1].SyncMetadata(slot));
    public void TruncateData(long length) => Both(d => d.TruncateData(length));
}
