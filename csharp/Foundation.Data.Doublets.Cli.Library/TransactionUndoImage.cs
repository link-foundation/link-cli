using System.Globalization;
using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

/// <summary>Undo source for an ordinary, immediately mutating storage adapter.</summary>
public enum RowUndoPolicy
{
    /// <summary>Post-effect observations leave a crash window before the callback.</summary>
    Observed,
    /// <summary>Flush a complete logical row/name image before an owner's first write.
    /// Adds O(N log N) capture and O(N + name bytes) memory/journal per owner.
    /// Each write scans rows/names and reconciles pending record effects.</summary>
    WriteAheadImage
}

// Mandatory v3 metadata uses the transition namespace: older readers reject it.
internal sealed record TransactionUndoImage<T>(Guid Owner, long Base, Link<T>[] Rows,
    IReadOnlyDictionary<T, string>? Names) where T : IUnsignedNumber<T>
{
    internal string Serialize()
    {
        static string Address(T value) => value.ToString(null, CultureInfo.InvariantCulture);
        var rows = string.Join(';', Rows.Select(row => string.Join(',', Address(row.Index), Address(row.Source), Address(row.Target))));
        var names = Names is null ? "-" : string.Join(';', Names.OrderBy(pair => pair.Key)
            .Select(pair => Address(pair.Key) + "," + JournalRecord<T>.EncodeName(pair.Value)));
        return $"v3|undo|{Owner:N}|{Base.ToString(CultureInfo.InvariantCulture)}|{rows}|{names}";
    }

    internal static TransactionUndoImage<T> Parse(string text)
    {
        static InvalidDataException Invalid() => new("Invalid write-ahead undo image.");
        static T Address(string text) => T.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out var value)
            ? value : throw Invalid();
        var parts = text.Split('|');
        if (parts.Length != 6 || parts[0] != "v3" || parts[1] != "undo"
            || !Guid.TryParseExact(parts[2], "N", out var owner) || owner == Guid.Empty
            || !long.TryParse(parts[3], NumberStyles.None, CultureInfo.InvariantCulture, out var start) || start < 0) throw Invalid();
        var rows = new List<Link<T>>();
        if (parts[4].Length != 0)
            foreach (var value in parts[4].Split(';'))
            {
                var fields = value.Split(',');
                if (fields.Length != 3) throw Invalid();
                var row = new Link<T>(Address(fields[0]), Address(fields[1]), Address(fields[2]));
                if (row.Index == T.Zero || (rows.Count != 0 && Comparer<T>.Default.Compare(rows[^1].Index, row.Index) >= 0)) throw Invalid();
                rows.Add(row);
            }
        Dictionary<T, string>? names = null;
        if (parts[5] != "-")
        {
            names = new();
            T previous = T.Zero;
            if (parts[5].Length != 0)
                foreach (var value in parts[5].Split(';'))
                {
                    var fields = value.Split(',');
                    if (fields.Length != 2 || !JournalRecord<T>.TryDecodeName(fields[1], out var name) || name is null) throw Invalid();
                    var index = Address(fields[0]);
                    if (names.Count != 0 && Comparer<T>.Default.Compare(previous, index) >= 0) throw Invalid();
                    names.Add(index, name); previous = index;
                }
        }
        var image = new TransactionUndoImage<T>(owner, start, rows.ToArray(), names);
        if (image.Serialize() != text) throw Invalid();
        return image;
    }
}
