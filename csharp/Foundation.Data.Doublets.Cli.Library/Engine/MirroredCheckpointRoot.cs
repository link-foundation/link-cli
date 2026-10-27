using static Foundation.Data.Doublets.Cli.Engine.PageFormat;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Selected two-copy checkpoint roots over exclusive caller-owned raw
/// devices. Selection never repairs. The checkpoint log validates its selected
/// bank before completing root recovery. Both copies synchronize before ACK.</summary>
public sealed class MirroredCheckpointRoot : IPageDevice
{
    private readonly IPageDevice[] _devices;
    private readonly uint _flags;
    private readonly SortedDictionary<int, Repair> _repairs = new();
    private bool _fenced;
    private sealed record Repair(byte[] Bytes, int Source, int[] Destinations);
    public MirroredCheckpointRoot(IPageDevice primary, IPageDevice secondary, uint bankFlags)
    {
        ArgumentNullException.ThrowIfNull(primary); ArgumentNullException.ThrowIfNull(secondary);
        if (ReferenceEquals(primary, secondary) || bankFlags > 1 || primary.MetadataFlags != 0 || secondary.MetadataFlags != 0)
            throw new ArgumentException("Checkpoint root mirroring requires distinct raw roots and bank mode 0 or 1.");
        _devices = new[] { primary, secondary }; _flags = 2 | bankFlags;
    }
    public uint MetadataFlags => _flags;
    public bool SupportsDurableFlush => _devices.All(d => d.SupportsDurableFlush);
    private void Check() { if (_fenced) throw Invalid("uncertain checkpoint root is fenced; reopen both devices"); }
    private void Slot(int slot) { Check(); if (slot is not (0 or 1)) throw Invalid("checkpoint root metadata slot"); }
    private void Mutate(Action action) { Check(); try { action(); } catch { _fenced = true; throw; } }
    private void Both(Action<IPageDevice> action) => Mutate(() => { foreach (var device in _devices) action(device); });
    public byte[] NewDatabaseId() { Check(); return _devices[0].NewDatabaseId(); }
    public long DataLength
    {
        get { Check(); foreach (var device in _devices) if (device.DataLength != 0) throw Invalid("checkpoint root data must be empty"); return 0; }
    }
    public byte[]? ReadMetadata(int slot)
    {
        Slot(slot);
        var raw = _devices.Select(d => { try { return d.ReadMetadata(slot); } catch (IOException) { return new byte[1]; } }).ToArray();
        var states = raw.Select(bytes => CheckpointDecision.Decode(bytes, slot, _flags)).ToArray();
        if (states[0] is { } a && states[1] is { } b && (!a.Identity.AsSpan().SequenceEqual(b.Identity)
            || Math.Max(a.Epoch, b.Epoch) - Math.Min(a.Epoch, b.Epoch) > 2)) throw Invalid("conflicting checkpoint root copies");
        if (states[0] is null && states[1] is null)
        {
            if (raw.Any(bytes => bytes is { Length: > 0 })) throw Invalid("both checkpoint root copies are unreadable");
            _repairs.Remove(slot); return null;
        }
        int source = states[0] is null || (states[1] is not null && states[1]!.Epoch > states[0]!.Epoch) ? 1 : 0;
        byte[] bytes = raw[source]!.ToArray();
        _repairs[slot] = new(bytes, source, Enumerable.Range(0, 2).Where(i => raw[i] is null || !raw[i]!.AsSpan().SequenceEqual(bytes)).ToArray());
        return bytes.ToArray();
    }
    public void CompleteRecovery() => Mutate(() =>
    {
        // Recheck every selected repair source before writing any destination.
        foreach (var (slot, repair) in _repairs)
        {
            if (repair.Destinations.Length == 0) continue;
            var current = _devices[repair.Source].ReadMetadata(slot);
            if (current is null || !current.AsSpan().SequenceEqual(repair.Bytes)) throw Invalid("checkpoint root repair source changed");
        }
        foreach (var (slot, repair) in _repairs) foreach (int side in repair.Destinations)
            { _devices[side].WriteMetadata(slot, repair.Bytes); _devices[side].SyncMetadata(slot); }
        _repairs.Clear();
    });
    public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes)
    {
        Slot(slot); byte[] copy = bytes.ToArray();
        if (CheckpointDecision.Decode(copy, slot, _flags) is null) throw Invalid("invalid checkpoint root publication");
        Both(d => d.WriteMetadata(slot, copy)); _repairs.Remove(slot);
    }
    public void SyncMetadata(int slot) { Slot(slot); Both(d => d.SyncMetadata(slot)); }
    public void SyncData() => Both(d => d.SyncData());
    public void TruncateData(long length) { Check(); if (length != 0) throw Invalid("checkpoint root data must be empty"); Both(d => d.TruncateData(0)); }
    public byte[] ReadPage(ulong index) { Check(); throw Invalid("checkpoint root has no data pages"); }
    public void WritePage(ulong index, ReadOnlySpan<byte> bytes) { Check(); throw Invalid("checkpoint root has no data pages"); }
}
