using System.Buffers.Binary;

namespace Foundation.Data.Doublets.Cli.Engine;

// Version-one wire representation. Independent vectors are under docs/protocol.
internal static class PageFormat
{
    internal const int PageSize = 4096;
    internal const ulong Cells = 252;
    internal const ulong PortableMax = (1UL << 53) - 1;
    internal const ulong MaxPages = PortableMax / PageSize;
    internal const ulong External = 1UL << 63;
    internal const ulong Payload = (1UL << 56) - 1;
    internal static ReadOnlySpan<byte> PageMagic => "LNKDPG01"u8;
    private static ReadOnlySpan<byte> MetaMagic => "LNKDMT01"u8;
    internal static IOException Invalid(string message) => new("Invalid native pages: " + message);
    internal static ulong U64(ReadOnlySpan<byte> bytes, int offset) => BinaryPrimitives.ReadUInt64LittleEndian(bytes[offset..]);
    internal static uint U32(ReadOnlySpan<byte> bytes, int offset) => BinaryPrimitives.ReadUInt32LittleEndian(bytes[offset..]);
    internal static void Put(Span<byte> bytes, int offset, ulong value) => BinaryPrimitives.WriteUInt64LittleEndian(bytes[offset..], value);
    internal static ulong Portable(ulong value) => value <= PortableMax ? value : throw Invalid("integer exceeds portable range");
    internal static ulong Hash(ReadOnlySpan<byte> bytes, ulong initial = 0xcbf29ce484222325)
    {
        foreach (byte value in bytes) initial = unchecked((initial ^ value) * 0x100000001b3);
        return initial;
    }
    internal static ulong PageHash(ReadOnlySpan<byte> page) => Hash(page[64..], Hash(page[..56]));

    internal sealed record Metadata(byte[] Identity, ulong Generation = 0, ulong Pages = 0, ulong Rows = 0,
        ulong Records = 0, ulong Root = 0, ulong LastHash = 0, uint Flags = 0)
    {
        internal byte[] Encode()
        {
            var bytes = new byte[128]; MetaMagic.CopyTo(bytes); BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(8), 1);
            BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(12), Flags);
            Identity.CopyTo(bytes, 16);
            var values = new[] { Generation, Pages, Rows, Records, Root, LastHash };
            for (int i = 0; i < values.Length; i++) Put(bytes, 32 + 8 * i, values[i]);
            Put(bytes, 120, Hash(bytes.AsSpan(0, 120))); return bytes;
        }
        internal static Metadata? Decode(byte[]? bytes, uint flags)
        {
            if (bytes is not { Length: 128 } || Hash(bytes.AsSpan(0, 120)) != U64(bytes, 120)) return null;
            if (!bytes.AsSpan(0, 8).SequenceEqual(MetaMagic) || U32(bytes, 8) != 1) throw Invalid("unsupported metadata version");
            if (U32(bytes, 12) != flags || bytes.AsSpan(80, 40).ContainsAnyExcept((byte)0)) throw Invalid("unsupported metadata flags");
            var state = new Metadata(bytes.AsSpan(16, 16).ToArray(), Portable(U64(bytes, 32)), Portable(U64(bytes, 40)),
                Portable(U64(bytes, 48)), Portable(U64(bytes, 56)), Portable(U64(bytes, 64)), U64(bytes, 72), flags);
            if (state.Pages > MaxPages || state.Generation > state.Pages || state.Rows > state.Pages * Cells || state.Records > state.Rows ||
                state.Rows < state.Pages || state.Records < state.Generation) throw Invalid("metadata counts");
            bool invalid = state.Pages == 0
                ? state.Generation != 0 || state.Rows != 0 || state.Records != 0 || state.Root != 0 || state.LastHash != 0
                : state.Generation == 0 || state.Root <= (state.Pages - 1) * Cells || state.Root > state.Pages * Cells;
            if (invalid) throw Invalid("metadata root");
            return state;
        }
    }
}
