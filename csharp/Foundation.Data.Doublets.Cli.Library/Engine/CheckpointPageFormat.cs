using System.Buffers.Binary;
using static Foundation.Data.Doublets.Cli.Engine.PageFormat;

namespace Foundation.Data.Doublets.Cli.Engine;

// Independent root decisions; existing bank page/metadata bytes stay unchanged.
internal sealed record CheckpointDecision(byte[] Identity, ulong Epoch, uint Flags)
{
    internal int Bank => (int)(Epoch % 2);
    internal byte[] BankIdentity()
    {
        var mask = new byte[16]; Put(mask, 0, Epoch); "BANK0001"u8.CopyTo(mask.AsSpan(8));
        return Identity.Select((value, i) => (byte)(value ^ mask[i])).ToArray();
    }
    internal byte[] Encode()
    {
        var bytes = new byte[128]; "CLNKBK01"u8.CopyTo(bytes); Identity.CopyTo(bytes, 8); Put(bytes, 24, Epoch);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(32), (uint)Bank);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(36), Flags); BankIdentity().CopyTo(bytes, 40);
        Put(bytes, 120, Hash(bytes.AsSpan(0, 120))); return bytes;
    }
    internal static CheckpointDecision? Decode(byte[]? bytes, int slot, uint flags)
    {
        if (bytes is not { Length: 128 } || Hash(bytes.AsSpan(0, 120)) != U64(bytes, 120)) return null;
        if (!bytes.AsSpan(0, 8).SequenceEqual("CLNKBK01"u8) || bytes.AsSpan(56, 64).ContainsAnyExcept((byte)0))
            throw Invalid("unsupported checkpoint root version or padding");
        var decision = new CheckpointDecision(bytes.AsSpan(8, 16).ToArray(), U64(bytes, 24), U32(bytes, 36));
        if (decision.Flags != flags || U32(bytes, 32) != (uint)decision.Bank || decision.Bank != slot ||
            !bytes.AsSpan(40, 16).SequenceEqual(decision.BankIdentity())) throw Invalid("checkpoint root flags, bank or epoch identity");
        return decision;
    }
    internal static CheckpointDecision? Select(byte[]?[] raw, uint flags)
    {
        var a = Decode(raw[0], 0, flags); var b = Decode(raw[1], 1, flags);
        if (a is null) return b; if (b is null) return a;
        if (!a.Identity.AsSpan().SequenceEqual(b.Identity) || Math.Max(a.Epoch, b.Epoch) - Math.Min(a.Epoch, b.Epoch) != 1)
            throw Invalid("conflicting checkpoint root decisions");
        return a.Epoch > b.Epoch ? a : b;
    }
}
