using System.Buffers.Binary;
using System.Text;
using static Foundation.Data.Doublets.Cli.Engine.PageFormat;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Caller-owned exclusive synchronous page device. Successful sync must
/// persist preceding file writes. A mutation failure requires reopening.</summary>
public interface IPageDevice
{
    uint MetadataFlags => 0;
    byte[] ReadCheckedPage(ulong index, ulong expected) => ReadPage(index);
    void CompleteRecovery() { }
    bool SupportsDurableFlush => false;
    byte[] NewDatabaseId();
    long DataLength { get; }
    byte[]? ReadMetadata(int slot);
    void WriteMetadata(int slot, ReadOnlySpan<byte> bytes);
    void SyncMetadata(int slot);
    byte[] ReadPage(ulong index);
    void WritePage(ulong index, ReadOnlySpan<byte> bytes);
    void SyncData();
    void TruncateData(long length);
}

/// <summary>Privileged repair of an existing page or contiguous missing suffix.
/// Validate the surviving copy and complete logical prefix before calling.</summary>
public interface IRepairPageDevice : IPageDevice
{
    void RepairPage(ulong index, ReadOnlySpan<byte> bytes);
}

/// <summary>Optional SQL-free immutable doublets pages with alternating metadata.
/// One caller owns the synchronous writer and closes its device separately.
/// Read/flush failures fence this log. Appends remain staged until flush.</summary>
public sealed class PagedCommitLog : ICommitLog
{
    public const int PageSize = PageFormat.PageSize;
    public const ulong MaximumPages = MaxPages;
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private readonly IPageDevice _device;
    private readonly ulong _maxPages;
    private Metadata _state;
    private readonly List<byte[]> _pending = new();
    private ulong _pendingRows;
    private bool _fenced;
    public bool SupportsDurableFlush => _device.SupportsDurableFlush;

    public PagedCommitLog(IPageDevice device, ulong maxPages = MaximumPages)
    {
        ArgumentNullException.ThrowIfNull(device);
        if (maxPages == 0 || maxPages > MaximumPages) throw new ArgumentOutOfRangeException(nameof(maxPages));
        _device = device; _maxPages = maxPages;
        uint flags = device.MetadataFlags;
        if (flags > 1) throw Invalid("unsupported device mode");
        var raw = new[] { device.ReadMetadata(0), device.ReadMetadata(1) };
        var first = Metadata.Decode(raw[0], flags); var second = Metadata.Decode(raw[1], flags);
        if (first is null && second is null)
        {
            if (raw.Any(bytes => bytes is { Length: > 0 }) || device.DataLength != 0) throw Invalid("no valid metadata decision");
            byte[] id = device.NewDatabaseId();
            if (id.Length != 16) throw Invalid("database identity length");
            _state = new Metadata((byte[])id.Clone(), Flags: flags);
            device.SyncData(); device.WriteMetadata(0, _state.Encode()); device.SyncMetadata(0);
        }
        else
        {
            if (first is not null && second is not null &&
                (!first.Identity.AsSpan().SequenceEqual(second.Identity) ||
                (first.Generation == second.Generation && !raw[0]!.AsSpan().SequenceEqual(raw[1]!)))) throw Invalid("conflicting metadata decisions");
            _state = first is null || (second is not null && second.Generation > first.Generation) ? second! : first;
        }
        if (_state.Pages > maxPages) throw Invalid("committed pages exceed selected limit");
        long length = device.DataLength, committed = checked((long)_state.Pages * PageSize);
        if (length < committed) throw Invalid("truncated committed pages");
        ReadCommitted(retain: false); device.CompleteRecovery();
        if (length > committed) { device.TruncateData(committed); device.SyncData(); }
    }
    private void Check()
    {
        if (_fenced) throw Invalid("uncertain native page owner is fenced; reopen the log");
    }
    public void Append(string record)
    {
        Check(); ArgumentNullException.ThrowIfNull(record);
        if (record.Contains('\r') || record.Contains('\n')) throw new ArgumentException("Journal records cannot contain line breaks.", nameof(record));
        byte[] bytes = Utf8.GetBytes(record);
        ulong rows = ((ulong)bytes.Length + 6) / 7 + 1;
        if (_state.Pages + (_pendingRows + rows + Cells - 1) / Cells > _maxPages) throw Invalid("native page address space exhausted");
        _pending.Add(bytes); _pendingRows += rows;
    }
    public IReadOnlyList<string> Read()
    {
        Check();
        try
        {
            var records = ReadCommitted(retain: true); _device.CompleteRecovery();
            records.AddRange(_pending.Select(bytes => Utf8.GetString(bytes))); return records;
        }
        catch { _fenced = true; throw; }
    }
    public void Flush()
    {
        Check(); if (_pending.Count == 0) return;
        _fenced = true;
        var writer = new Writer(_device, _state with { Generation = _state.Generation + 1 });
        foreach (var bytes in _pending)
        {
            ulong text = 0;
            for (int end = bytes.Length; end > 0;)
            {
                int start = (end - 1) / 7 * 7, count = end - start;
                ulong payload = 0;
                for (int i = 0; i < count; i++) payload |= (ulong)bytes[start + i] << (8 * i);
                text = writer.Cell(External | ((ulong)count << 56) | payload, text); end = start;
            }
            ulong root = writer.Cell(text, writer.State.Root);
            writer.State = writer.State with { Root = root, Records = writer.State.Records + 1 };
        }
        writer.Finish(); _device.SyncData();
        var state = writer.State; int slot = (int)(state.Generation % 2);
        _device.WriteMetadata(slot, state.Encode()); _device.SyncMetadata(slot);
        _state = state; _pending.Clear(); _pendingRows = 0; _fenced = false;
    }
    private sealed class Writer(IPageDevice device, Metadata state)
    {
        internal Metadata State = state;
        private readonly byte[] _page = new byte[PageSize];
        private int _count;
        internal ulong Cell(ulong source, ulong target)
        {
            if (_count == (int)Cells) Finish();
            ulong id = State.Pages * Cells + (ulong)_count + 1;
            Put(_page, 64 + _count * 16, source); Put(_page, 72 + _count * 16, target);
            _count++; State = State with { Rows = State.Rows + 1 }; return id;
        }
        internal void Finish()
        {
            PageMagic.CopyTo(_page); State.Identity.CopyTo(_page, 8);
            Put(_page, 24, State.Pages); Put(_page, 32, State.Generation);
            BinaryPrimitives.WriteUInt32LittleEndian(_page.AsSpan(40), (uint)_count); Put(_page, 48, State.LastHash);
            ulong hash = PageHash(_page); Put(_page, 56, hash); device.WritePage(State.Pages, _page);
            State = State with { Pages = State.Pages + 1, LastHash = hash }; Array.Clear(_page); _count = 0;
        }
    }
    internal sealed class Cursor(IPageDevice device, Metadata state, ulong? startingHash = null)
    {
        private long _index = (long)state.Pages - 1;
        private ulong _generation = state.Generation, _expected = startingHash ?? state.LastHash;
        private byte[] _page = Array.Empty<byte>();
        private int _count;
        internal void StartAt(ulong root) { _index = (long)((root - 1) / Cells); Load(); _count = (int)((root - 1) % Cells + 1); }
        internal ulong Consumed { get; private set; }
        internal ulong Current => _index < 0 ? 0 : (ulong)_index * Cells + (ulong)_count;
        internal void Load()
        {
            _page = device.ReadCheckedPage((ulong)_index, _expected);
            if (_page.Length != PageSize) throw Invalid("page length");
            ulong actual = PageHash(_page);
            if (!_page.AsSpan(0, 8).SequenceEqual(PageMagic) || !_page.AsSpan(8, 16).SequenceEqual(state.Identity) ||
                U64(_page, 24) != (ulong)_index || actual != _expected || actual != U64(_page, 56)) throw Invalid("page checksum or identity");
            ulong created = U64(_page, 32);
            if (created == 0 || created > _generation || created < _generation - 1 ||
                ((ulong)_index == state.Pages - 1 && created != _generation) || (_index == 0 && created != 1)) throw Invalid("page generation");
            _generation = created; uint count = U32(_page, 40); _expected = U64(_page, 48);
            if (count == 0 || count > Cells || U32(_page, 44) != 0 || _page.AsSpan(64 + (int)count * 16).ContainsAnyExcept((byte)0) ||
                (_index == 0 && _expected != 0)) throw Invalid("page count or padding");
            _count = (int)count;
        }
        internal (ulong Source, ulong Target) Take(ulong wanted)
        {
            if (wanted == 0 || wanted != Current) throw Invalid("noncanonical doublet reference");
            int offset = 64 + (_count - 1) * 16;
            var result = (U64(_page, offset), Portable(U64(_page, offset + 8))); Consumed++; _count--;
            if (_count == 0 && --_index >= 0) Load();
            return result;
        }
    }
    private List<string> ReadCommitted(bool retain) => ReadCommitted(_device, _state, retain);
    internal static List<string> ReadCommitted(IPageDevice device, Metadata state, bool retain)
    {
        var records = new List<string>(); if (state.Pages == 0) return records;
        var cursor = new Cursor(device, state); cursor.Load(); ulong root = state.Root, count = 0;
        while (root != 0)
        {
            var (source, previous) = cursor.Take(root); ulong text = Portable(source);
            using var bytes = new MemoryStream();
            while (text != 0)
            {
                var (external, next) = cursor.Take(text); int length = (int)((external >> 56) & 7); ulong payload = external & Payload;
                if ((external & External) == 0 || (external & 0x7800000000000000) != 0 || length == 0 ||
                    payload >> (length * 8) != 0 || (next != 0 && length != 7)) throw Invalid("string doublet");
                for (int i = 0; i < length; i++) bytes.WriteByte((byte)(payload >> (i * 8)));
                text = next;
            }
            string record;
            try { record = Utf8.GetString(bytes.GetBuffer(), 0, checked((int)bytes.Length)); }
            catch (DecoderFallbackException cause) { throw new IOException("Invalid native pages: record UTF-8", cause); }
            if (record.Contains('\r') || record.Contains('\n')) throw Invalid("record delimiter");
            if (previous != cursor.Current) throw Invalid("record predecessor");
            if (retain) records.Add(record);
            root = previous; count++;
        }
        if (cursor.Current != 0 || cursor.Consumed != state.Rows || count != state.Records) throw Invalid("record counts");
        records.Reverse(); return records;
    }
}
