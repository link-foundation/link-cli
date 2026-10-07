using System.Text;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// A whole links store in binary links notation: two <see cref="LinksPacket"/>s,
/// one after the other.
/// </summary>
/// <remarks>
/// <list type="number">
/// <item><b>Links</b>, without external references: the store link
/// <c>(a: s t)</c> is the doublet <c>s t</c> at address <c>a</c>. Addresses the
/// store does not use are holes, so every link keeps its address.</item>
/// <item><b>Names</b>, with external references: one link per named store link,
/// in address order, holding the external values <c>address code-point…</c>.</item>
/// </list>
/// Both packets use packed widths, so a store of small addresses costs about
/// two bytes per link. <see cref="Import"/> writes every link back at its own
/// address and restores the names, so exporting the imported store gives the
/// same bytes again. The Rust <c>protocol::archive</c> module writes the same bytes.
/// </remarks>
public static class StoreArchive
{
    /// <summary>Writes every link and name of <paramref name="links"/> to <paramref name="output"/>.</summary>
    public static void Export(INamedTypesLinks<uint> links, Stream output)
    {
        ArgumentNullException.ThrowIfNull(links);
        ArgumentNullException.ThrowIfNull(output);
        var any = links.Constants.Any;
        var stored = links
            .All(new DoubletLink(any, any, any))
            .Select(link => new DoubletLink(link))
            .OrderBy(link => link.Index)
            .ToList();
        var doublets = stored
            .Select(link => ((ulong)link.Index, new[]
            {
                PacketReference.Internal(link.Source),
                PacketReference.Internal(link.Target),
            }))
            .ToList();
        var names = new List<(ulong Address, PacketReference[] References)>();
        foreach (var link in stored)
        {
            var name = links.GetName(link.Index);
            if (name is not null)
            {
                var references = new List<PacketReference> { PacketReference.External(link.Index) };
                references.AddRange(name.EnumerateRunes().Select(rune => PacketReference.External((ulong)rune.Value)));
                names.Add(((ulong)names.Count + 1, references.ToArray()));
            }
        }
        output.Write(LinoProtocolException.Wrap(() => LinksPacket.Pack(false, doublets, true).ToBytes()));
        output.Write(LinoProtocolException.Wrap(() => LinksPacket.Pack(true, names, true).ToBytes()));
    }

    /// <summary>Writes the archive of <paramref name="links"/> to the file at <paramref name="path"/>.</summary>
    public static void ExportToFile(INamedTypesLinks<uint> links, string path)
    {
        using var output = new MemoryStream();
        Export(links, output);
        File.WriteAllBytes(path, output.ToArray());
    }

    /// <summary>
    /// Reads an archive from <paramref name="input"/> into <paramref name="links"/>:
    /// each link is written at its own address, then every name is set.
    /// </summary>
    public static void Import(INamedTypesLinks<uint> links, Stream input)
    {
        ArgumentNullException.ThrowIfNull(links);
        ArgumentNullException.ThrowIfNull(input);
        var reader = new PacketReader(input);
        var linksPacket = ReadPacket(reader, "links");
        var namesPacket = ReadPacket(reader, "names");
        if (!LinoProtocolException.Wrap(() => reader.AtEnd))
        {
            throw LinoProtocolException.Malformed("trailing bytes after the store archive");
        }
        var doublets = linksPacket.Links().Select(link => DecodeDoublet(link.Address, link.References)).ToList();
        var names = namesPacket.Links().Select(link => DecodeName(link.References)).ToList();
        // Every address exists before any link refers to it.
        foreach (var (index, _, _) in doublets)
        {
            if (!links.Exists(index))
            {
                LinksExtensions.EnsureCreated(links, index);
            }
        }
        foreach (var (index, source, target) in doublets)
        {
            LinoDatabaseInput.UpdateLink(links, index, source, target);
        }
        foreach (var (index, name) in names)
        {
            links.SetName(index, name);
        }
    }

    /// <summary>Reads the archive in the file at <paramref name="path"/> into <paramref name="links"/>.</summary>
    public static void ImportFromFile(INamedTypesLinks<uint> links, string path)
    {
        using var input = File.OpenRead(path);
        Import(links, input);
    }

    private static LinksPacket ReadPacket(PacketReader reader, string part) =>
        LinoProtocolException.Wrap(() => LinksPacket.ReadFrom(reader, DecodeLimits.Unlimited))
        ?? throw LinoProtocolException.Malformed($"the store archive ends before its {part}");

    private static (uint Index, uint Source, uint Target) DecodeDoublet(ulong address, PacketReference[] references)
    {
        if (references is not [{ IsExternal: false } source, { IsExternal: false } target])
        {
            throw LinoProtocolException.Malformed($"archive link {address} is not a doublet of link addresses");
        }
        return (StoreAddress(address), StoreAddress(source.Value), StoreAddress(target.Value));
    }

    private static (uint Index, string Name) DecodeName(PacketReference[] references)
    {
        if (references.Any(reference => !reference.IsExternal))
        {
            throw LinoProtocolException.Malformed("an archive name holds only external values");
        }
        var name = new StringBuilder();
        foreach (var codePoint in references.Skip(1).Select(reference => reference.Value))
        {
            if (codePoint > int.MaxValue || !Rune.IsValid((int)codePoint))
            {
                throw LinoProtocolException.Malformed($"invalid code point {codePoint}");
            }
            name.Append(new Rune((int)codePoint).ToString());
        }
        return (StoreAddress(references[0].Value), name.ToString());
    }

    private static uint StoreAddress(ulong address) =>
        address <= uint.MaxValue
            ? (uint)address
            : throw LinoProtocolException.Malformed($"address {address} does not fit a 32-bit store");
}
