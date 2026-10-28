using System.Globalization;
using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public readonly record struct NameChange<TLinkAddress>(TLinkAddress Index, string? Before, string? After)
    where TLinkAddress : IUnsignedNumber<TLinkAddress>;

/// <summary>Complete legacy journal payload. Row-only values retain the v1 codec.</summary>
public sealed class JournalRecord<TLinkAddress> : IEquatable<JournalRecord<TLinkAddress>>
    where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    public Transition<TLinkAddress> Transition { get; }
    public bool NameOnly { get; }
    public IReadOnlyList<NameChange<TLinkAddress>> Names { get; }
    public Guid TransactionId => Transition.TransactionId;
    public long Sequence => Transition.Sequence;
    public DateTimeOffset Timestamp => Transition.Timestamp;
    public TransitionKind Kind => Transition.Kind;
    public Link<TLinkAddress> Before => Transition.Before;
    public Link<TLinkAddress> After => Transition.After;

    public JournalRecord(Transition<TLinkAddress> transition, bool nameOnly = false,
        IEnumerable<NameChange<TLinkAddress>>? names = null)
    {
        Transition = transition;
        NameOnly = nameOnly;
        Names = Array.AsReadOnly(names?.ToArray() ?? []);
    }

    public bool Equals(JournalRecord<TLinkAddress>? other) => other is not null
        && Transition.Equals(other.Transition) && NameOnly == other.NameOnly && Names.SequenceEqual(other.Names);
    public override bool Equals(object? other) => other is JournalRecord<TLinkAddress> record && Equals(record);
    public override int GetHashCode()
    {
        var hash = new HashCode(); hash.Add(Transition); hash.Add(NameOnly);
        foreach (var name in Names) hash.Add(name);
        return hash.ToHashCode();
    }

    public string Serialize()
    {
        var row = Transition.Serialize();
        if (!NameOnly && Names.Count == 0) return row;
        var names = string.Join(';', Names.Select(name => string.Join(',',
            name.Index.ToString(null, CultureInfo.InvariantCulture), EncodeName(name.Before), EncodeName(name.After))));
        return "v2" + row[2..] + "|" + (NameOnly ? "n" : "r") + "|" + names;
    }

    public static bool TryParse(string text, out JournalRecord<TLinkAddress> record)
    {
        record = null!;
        if (text is null) return false;
        if (!text.StartsWith("v2|", StringComparison.Ordinal))
        {
            if (!Transition<TLinkAddress>.TryParse(text, out var row)) return false;
            record = new(row); return true;
        }
        var parts = text.Split('|');
        if (parts.Length != 9 || !Transition<TLinkAddress>.TryParse("v1|" + string.Join('|', parts[1..7]), out var transition)
            || (parts[7] != "n" && parts[7] != "r")) return false;
        var names = new List<NameChange<TLinkAddress>>();
        foreach (var change in parts[8].Split(';'))
        {
            var fields = change.Split(',');
            if (fields.Length != 3 || !TLinkAddress.TryParse(fields[0], NumberStyles.None, CultureInfo.InvariantCulture, out var index)
                || !TryDecodeName(fields[1], out var before) || !TryDecodeName(fields[2], out var after)) return false;
            names.Add(new(index, before, after));
        }
        var parsed = new JournalRecord<TLinkAddress>(transition, parts[7] == "n", names);
        if (!parsed.IsValid() || parsed.Serialize() != text) return false;
        record = parsed; return true;
    }

    internal bool IsValid()
    {
        for (var i = 1; i < Names.Count; i++) if (Comparer<TLinkAddress>.Default.Compare(Names[i - 1].Index, Names[i].Index) >= 0) return false;
        if (NameOnly) return Names.Count != 0 && Kind == TransitionKind.Update
            && Before.Equals(default(Link<TLinkAddress>)) && After.Equals(default(Link<TLinkAddress>));
        var index = After.Index == TLinkAddress.Zero ? Before.Index : After.Index;
        var shape = Kind switch
        {
            TransitionKind.Create => Before.Equals(default(Link<TLinkAddress>)) && After.Index != TLinkAddress.Zero,
            TransitionKind.Update => Before.Index != TLinkAddress.Zero && Before.Index == After.Index,
            TransitionKind.Delete => Before.Index != TLinkAddress.Zero && After.Equals(default(Link<TLinkAddress>)),
            _ => false,
        };
        return Names.Count == 0 || (shape && Names.Count == 1 && Names[0].Index == index && index != TLinkAddress.Zero);
    }

    internal static string EncodeName(string? name) => name is null ? "-"
        : string.Concat(name.Select(unit => ((ushort)unit).ToString("X4", CultureInfo.InvariantCulture)));

    internal static bool TryDecodeName(string text, out string? name)
    {
        name = null;
        if (text == "-") return true;
        if (text.Length % 4 != 0) return false;
        var units = new char[text.Length / 4];
        for (var i = 0; i < units.Length; i++)
        {
            var digits = text.AsSpan(i * 4, 4);
            foreach (var digit in digits) if (!Uri.IsHexDigit(digit)) return false;
            if (!ushort.TryParse(digits, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var unit)) return false;
            units[i] = (char)unit;
        }
        name = new string(units); return true;
    }
}
