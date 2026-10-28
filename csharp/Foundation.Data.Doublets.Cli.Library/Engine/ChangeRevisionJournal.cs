using System.Globalization;
using System.Numerics;
using System.Text;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Canonical logical engine2 redo bound to its exact predecessor.</summary>
internal static class ChangeRevisionJournal<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private static string Number<TNumber>(TNumber value) where TNumber : IFormattable => RevisionJournal<T>.Number(value);
    internal static string Encode(EngineSnapshot<T> snapshot, BigInteger next, EngineSnapshot<T> previous, IEnumerable<T> writes)
    {
        var operations = new List<string>();
        foreach (var id in writes.Distinct().Order())
        {
            EngineRow<T>? before = previous.Rows.TryGetValue(id, out var old) ? old : null;
            EngineRow<T>? after = snapshot.Rows.TryGetValue(id, out var current) ? current : null;
            if (before == after) continue;
            operations.Add(after is { } row
                ? $"+{Number(id)},{Number(row.Link.Source)},{Number(row.Link.Target)},{(row.Name is null ? "-" : Convert.ToHexStringLower(RevisionJournal<T>.Utf8.GetBytes(row.Name)))}"
                : $"-{Number(id)}");
        }
        var text = $"engine2|{Number(snapshot.Revision)}|{Number(previous.Revision)}|{Number(next)}|{string.Join(";", operations)}";
        return $"{text}|{RevisionJournal<T>.Checksum(text):x16}";
    }
    private static string Decimal(string text) => text.Length > 0 && text.All(c => c is >= '0' and <= '9')
        && (text.Length == 1 || text[0] != '0') ? text : throw new FormatException("Noncanonical change decimal.");
    private static T Address(string text) => RevisionJournal<T>.Address(Decimal(text));
    private static bool LowerHex(string text) => text.All(c => c is >= '0' and <= '9' or >= 'a' and <= 'f');
    internal static (EngineSnapshot<T>, BigInteger) Decode(string record, EngineSnapshot<T> previous, StateStructure structure)
    {
        try
        {
            var text = RevisionJournal<T>.CheckedText(record);
            if (!LowerHex(record[(record.LastIndexOf('|') + 1)..])) throw new FormatException("Noncanonical change checksum.");
            var parts = text.Split('|');
            if (parts.Length != 5 || parts[0] != "engine2") throw new FormatException("Unsupported changes format.");
            var revision = ulong.Parse(Decimal(parts[1]), NumberStyles.None, CultureInfo.InvariantCulture);
            var baseRevision = ulong.Parse(Decimal(parts[2]), NumberStyles.None, CultureInfo.InvariantCulture);
            var next = BigInteger.CreateChecked(UInt128.Parse(Decimal(parts[3]), NumberStyles.None, CultureInfo.InvariantCulture));
            if (baseRevision != previous.Revision || next < 1) throw new FormatException("Change base or allocator mismatch.");
            var rows = new EngineRows<T>(previous.Rows, structure); T? last = null;
            if (parts[4].Length > 0)
                foreach (var operation in parts[4].Split(';'))
                {
                    T id; EngineRow<T>? row;
                    if (operation.StartsWith('+'))
                    {
                        var fields = operation[1..].Split(',');
                        if (fields.Length != 4) throw new FormatException("Invalid change row.");
                        id = Address(fields[0]); string? name = null;
                        if (fields[3] != "-")
                        {
                            if (!LowerHex(fields[3])) throw new FormatException("Noncanonical change name.");
                            name = RevisionJournal<T>.Utf8.GetString(Convert.FromHexString(fields[3]));
                        }
                        row = new(new Link<T>(id, Address(fields[1]), Address(fields[2])), name);
                    }
                    else if (operation.StartsWith('-')) { id = Address(operation[1..]); row = null; }
                    else throw new FormatException("Unknown change operation.");
                    if (id == T.Zero || BigInteger.CreateChecked(id) >= next || (last is { } prior && prior >= id))
                        throw new FormatException("Change addresses must be ordered, unique and inside allocator.");
                    last = id;
                    EngineRow<T>? before = rows.TryGetValue(id, out var present) ? present : null;
                    if (before == row) throw new FormatException("Change has no row effect.");
                    if (row is { } value) rows.Set(id, value); else rows.Remove(id);
                }
            return (EngineSnapshot<T>.Create(revision, rows), next);
        }
        catch (Exception error) when (error is FormatException or OverflowException or DecoderFallbackException)
        { throw new InvalidDataException("Malformed engine change journal.", error); }
    }
}
