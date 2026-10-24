using System.Globalization;
using System.Numerics;
using System.Text;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>The engine1 wire format is shared with the Rust engine.</summary>
internal static class RevisionJournal<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private static string Number<TNumber>(TNumber value) where TNumber : IFormattable => value.ToString(null, CultureInfo.InvariantCulture);
    internal static ulong Checksum(string text)
    {
        ulong hash = 0xcbf29ce484222325;
        foreach (byte value in Utf8.GetBytes(text)) hash = unchecked((hash ^ value) * 0x100000001b3);
        return hash;
    }
    internal static string Encode(EngineSnapshot<T> snapshot, BigInteger next)
    {
        var rows = string.Join(";", snapshot.Rows.Values.Select(row =>
            $"{Number(row.Link.Index)},{Number(row.Link.Source)},{Number(row.Link.Target)},{(row.Name is null ? "-" : Convert.ToHexStringLower(Utf8.GetBytes(row.Name)))}"));
        var text = $"engine1|{Number(snapshot.Revision)}|{Number(next)}|{rows}";
        return $"{text}|{Checksum(text):x16}";
    }
    private static string Decimal(string text) => text.Length > 0 && text.All(c => c is >= '0' and <= '9')
        ? text : throw new FormatException("Expected ASCII decimal digits.");
    private static T Address(string text) => T.CreateChecked(UInt128.Parse(Decimal(text), NumberStyles.None, CultureInfo.InvariantCulture));
    internal static (EngineSnapshot<T>, BigInteger) Decode(string record)
    {
        try
        {
            int separator = record.LastIndexOf('|');
            if (separator < 0 || record.Length - separator != 17 || !record[(separator + 1)..].All(char.IsAsciiHexDigit))
                throw new FormatException("Missing or invalid checksum.");
            string text = record[..separator];
            if (ulong.Parse(record[(separator + 1)..], NumberStyles.HexNumber, CultureInfo.InvariantCulture) != Checksum(text))
                throw new FormatException("Checksum mismatch.");
            var parts = text.Split('|');
            if (parts.Length != 4 || parts[0] != "engine1") throw new FormatException("Unsupported journal format.");
            ulong revision = ulong.Parse(Decimal(parts[1]), NumberStyles.None, CultureInfo.InvariantCulture);
            BigInteger next = BigInteger.CreateChecked(UInt128.Parse(Decimal(parts[2]), NumberStyles.None, CultureInfo.InvariantCulture));
            if (next < 1) throw new FormatException("Invalid allocator.");
            var rows = new SortedDictionary<T, EngineRow<T>>();
            if (parts[3].Length > 0)
            {
                foreach (var row in parts[3].Split(';'))
                {
                    var fields = row.Split(',');
                    if (fields.Length != 4) throw new FormatException("Invalid row.");
                    T id = Address(fields[0]);
                    if (id == T.Zero || BigInteger.CreateChecked(id) >= next) throw new FormatException("Invalid address.");
                    string? name = fields[3] == "-" ? null : Utf8.GetString(Convert.FromHexString(fields[3]));
                    if (!rows.TryAdd(id, new(new Link<T>(id, Address(fields[1]), Address(fields[2])), name)))
                        throw new FormatException("Duplicate address.");
                }
            }
            return (new(revision, rows), next);
        }
        catch (Exception error) when (error is FormatException or OverflowException or DecoderFallbackException)
        { throw new InvalidDataException("Malformed engine revision journal.", error); }
    }
}
