using System.Diagnostics.CodeAnalysis;
using System.Globalization;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// How many references the links of a <see cref="Section"/> hold:
/// <c>Min..=Max</c>, where a null <see cref="Max"/> means no upper bound.
/// </summary>
/// <param name="Min">Fewest references in a link, at least 1.</param>
/// <param name="Max">Most references in a link, null for no limit.</param>
public readonly record struct ArityRange(ulong Min, ulong? Max)
{
    /// <summary>The largest minimum arity a section shape can hold.</summary>
    public const ulong MaxMin = ulong.MaxValue >> 4;

    /// <summary>Links of exactly two references.</summary>
    public static ArityRange Doublets { get; } = Exactly(2);

    /// <summary>Links of exactly <paramref name="arity"/> references.</summary>
    public static ArityRange Exactly(ulong arity) => new(arity, arity);

    /// <summary>Links of at least <paramref name="min"/> references.</summary>
    public static ArityRange AtLeast(ulong min) => new(min, null);

    /// <summary>Links of <c>min..=max</c> references.</summary>
    public static ArityRange Between(ulong min, ulong max) => new(min, max);

    /// <summary>True when a link of <paramref name="length"/> references fits the range.</summary>
    public bool Contains(ulong length) => length >= Min && (Max is not { } max || length <= max);

    /// <summary>True when every link has the same number of references, so links need no length prefix.</summary>
    public bool IsFixed => Max == Min;

    /// <summary>0 for no maximum, otherwise <c>Max - Min</c>; only for variable arities.</summary>
    internal ulong Extra => Max is { } max ? max - Min : 0;

    /// <summary>Null for a usable range, otherwise why it is not.</summary>
    internal string? Problem()
    {
        if (Min == 0)
        {
            return "arity must be at least 1";
        }
        if (Min > MaxMin)
        {
            return $"arity {Min} is too large";
        }
        if (Max is { } max && max < Min)
        {
            return $"arity range {this} is empty";
        }
        return null;
    }

    /// <summary>The range a section header describes; <paramref name="extra"/> 0 means no maximum.</summary>
    internal static ArityRange FromShape(ulong min, ulong? extra)
    {
        ArityRange range;
        if (extra is not { } added)
        {
            range = Exactly(min);
        }
        else if (added == 0)
        {
            range = AtLeast(min);
        }
        else
        {
            if (min > ulong.MaxValue - added)
            {
                throw LinoProtocolException.Malformed("arity range overflows 64 bits");
            }
            range = Between(min, min + added);
        }
        if (range.Problem() is { } problem)
        {
            throw LinoProtocolException.Malformed(problem);
        }
        return range;
    }

    /// <summary>Formats the range as <c>n</c>, <c>min..max</c> or <c>min..</c>.</summary>
    public override string ToString() => Max switch
    {
        { } max when max == Min => max.ToString(CultureInfo.InvariantCulture),
        { } max => $"{Min}..{max}",
        null => $"{Min}..",
    };

    /// <summary>Parses <c>n</c>, <c>min..max</c> (inclusive) or <c>min..</c> (no maximum).</summary>
    /// <exception cref="FormatException">The text is not a usable arity range.</exception>
    public static ArityRange Parse(string text) =>
        TryParse(text, out var range, out var error) ? range : throw new FormatException(error);

    /// <summary>Parses <c>n</c>, <c>min..max</c> (inclusive) or <c>min..</c> (no maximum).</summary>
    public static bool TryParse([NotNullWhen(true)] string? text, out ArityRange range) =>
        TryParse(text, out range, out _);

    /// <summary>Parses an arity range, explaining in <paramref name="error"/> why it failed.</summary>
    public static bool TryParse([NotNullWhen(true)] string? text, out ArityRange range, [NotNullWhen(false)] out string? error)
    {
        range = default;
        error = $"invalid arity '{text}': expected n, min..max or min..";
        if (text is null)
        {
            return false;
        }
        var separator = text.IndexOf("..", StringComparison.Ordinal);
        if (separator < 0)
        {
            if (!TryParseNumber(text, out var arity))
            {
                return false;
            }
            range = Exactly(arity);
        }
        else
        {
            var maxText = text[(separator + 2)..];
            if (!TryParseNumber(text[..separator], out var min))
            {
                return false;
            }
            if (maxText.Length == 0)
            {
                range = AtLeast(min);
            }
            else if (TryParseNumber(maxText, out var max))
            {
                range = Between(min, max);
            }
            else
            {
                return false;
            }
        }
        error = range.Problem();
        return error is null;
    }

    // Like Rust's u64::from_str: ASCII digits with an optional leading '+'.
    private static bool TryParseNumber(string text, out ulong value)
    {
        var digits = text.StartsWith('+') ? text[1..] : text;
        return ulong.TryParse(digits, NumberStyles.None, CultureInfo.InvariantCulture, out value);
    }
}
