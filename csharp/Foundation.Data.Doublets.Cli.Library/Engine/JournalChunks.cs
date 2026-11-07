using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Optional immutable journal chunk write limits. Readers accept both
/// whole-journal and chunk manifests independently of this write selection.</summary>
public sealed record JournalChunkOptions(int MaxRecords = 64, int MaxBytes = 65536, int MaxChunks = 1024);

internal static class JournalChunks
{
    private static readonly UTF8Encoding StrictUtf8 = new(false, true);
    internal sealed record Reference(string Key, ulong Generation, int? Records);
    internal sealed record Manifest(string Partition, Reference[] References);
    internal static void Validate(JournalChunkOptions options)
    {
        if (options.MaxRecords is < 1 or > 65536 || options.MaxBytes is < 1 or > 8388608 || options.MaxChunks is < 1 or > 4096)
            throw new ArgumentOutOfRangeException(nameof(options), "Invalid journal chunk limits.");
    }
    internal static IReadOnlyList<string>[] Split(string[] records, JournalChunkOptions options)
    {
        var chunks = new List<IReadOnlyList<string>>(); var current = new List<string>(); var bytes = 0;
        foreach (var record in records)
        {
            var length = StrictUtf8.GetByteCount(record);
            if (length > options.MaxBytes) throw new ArgumentException("Chunk record exceeds byte limit.", nameof(records));
            if (current.Count > 0 && (current.Count == options.MaxRecords || length > options.MaxBytes - bytes))
            {
                chunks.Add(current.ToArray()); current.Clear(); bytes = 0;
            }
            if (chunks.Count >= options.MaxChunks) throw new ArgumentException("Journal chunk count limit.", nameof(records));
            current.Add(record); bytes += length;
        }
        if (current.Count > 0) chunks.Add(current.ToArray());
        return chunks.ToArray();
    }
    private static bool Decimal(string value, ulong maximum, out ulong number) => ulong.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out number) && number <= maximum && number.ToString(CultureInfo.InvariantCulture) == value;
    private static bool Address(string value) => Regex.IsMatch(value, "\\Ablob1-[0-9a-f]{16}\\z", RegexOptions.CultureInvariant);
    private static InvalidDataException Invalid() => new("Invalid catalog pointer or chunk manifest.");
    internal static Manifest Decode(IReadOnlyList<string> parts)
    {
        if (parts.Any(value => value is null)) throw Invalid();
        if (parts.Count == 4 && parts[0] == "blob1" && Address(parts[2]) && Decimal(parts[3], ulong.MaxValue, out var generation) && generation > 0)
            return new(parts[1], new[] { new Reference(parts[2], generation, null) });
        if (parts.Count < 4 || parts[0] != "chunks1" || !Decimal(parts[2], uint.MaxValue, out var total) || !Decimal(parts[3], 4096, out var count) || parts.Count != 4 + (int)count * 3) throw Invalid();
        var references = new List<Reference>(); ulong sum = 0;
        for (var i = 4; i < parts.Count; i += 3)
        {
            if (!Address(parts[i]) || !Decimal(parts[i + 1], ulong.MaxValue, out generation) || generation == 0 || !Decimal(parts[i + 2], 65536, out var records) || records == 0) throw Invalid();
            sum += records; references.Add(new(parts[i], generation, (int)records));
        }
        if (sum != total) throw Invalid();
        return new(parts[1], references.ToArray());
    }
    internal static void ValidateChunk(IReadOnlyList<string> records, int? count)
    {
        if (count is null) return;
        if (records.Count != count) throw new InvalidDataException("Invalid immutable chunk count.");
        long bytes = 0;
        try
        {
            foreach (var record in records)
            {
                bytes += StrictUtf8.GetByteCount(record);
                if (bytes > 8388608) throw new InvalidDataException("Invalid immutable chunk byte bound.");
            }
        }
        catch (EncoderFallbackException error) { throw new InvalidDataException("Invalid immutable chunk Unicode.", error); }
    }
}
