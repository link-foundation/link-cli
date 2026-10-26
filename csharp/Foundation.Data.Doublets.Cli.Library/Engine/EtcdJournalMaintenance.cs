using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class EtcdJournalStore
{
    private static class NamespaceCodec
    {
        // Explicit initialization keeps ordinary coordinator calls from allocating
        // maintenance encoding state when this optional capability is unused.
        static NamespaceCodec() { }
        internal static readonly UTF8Encoding Encoding = new(false, true);
    }
    private static string NamespaceDecode(JsonElement value)
    {
        var encoded = value.GetString() ?? throw new InvalidDataException("Missing namespace base64.");
        var bytes = Convert.FromBase64String(encoded);
        if (Convert.ToBase64String(bytes) != encoded) throw new InvalidDataException("Noncanonical namespace base64.");
        return NamespaceCodec.Encoding.GetString(bytes);
    }
    public IReadOnlyList<JournalNamespaceEntry> ScanNamespace(JournalScanOptions? options = null)
    {
        options ??= new();
        if (options.MaxKeys <= 0 || options.MaxBytes <= 0 || options.PageKeys is < 1 or > 256)
            throw new ArgumentOutOfRangeException(nameof(options));
        Exception? failure = null;
        for (var attempt = 0; attempt < _endpoints.Length; attempt++)
        {
            try { return ScanOnce(options); }
            catch (Exception cause) { failure = cause; _cursor = (_cursor + 1) % _endpoints.Length; }
        }
        throw new IOException("Complete namespace snapshot failed.", failure);
    }
    private IReadOnlyList<JournalNamespaceEntry> ScanOnce(JournalScanOptions options)
    {
        var entries = new List<JournalNamespaceEntry>();
        long bytes = 0; ulong? revision = null;
        var previous = _prefix; var start = previous;
        var end = Convert.ToBase64String(NamespaceCodec.Encoding.GetBytes(_prefix[..^1] + "0"));
        while (true)
        {
            var limit = (int)Math.Min(options.PageKeys, (long)options.MaxKeys - entries.Count + 1);
            var range = new Dictionary<string, object>
            {
                ["key"] = Convert.ToBase64String(NamespaceCodec.Encoding.GetBytes(start)),
                ["range_end"] = end,
                ["limit"] = limit.ToString(CultureInfo.InvariantCulture),
                ["sort_order"] = "ASCEND",
                ["sort_target"] = "KEY",
                ["keys_only"] = options.KeysOnly
            };
            if (revision is not null) range["revision"] = revision.Value.ToString(CultureInfo.InvariantCulture);
            var response = Post(new { success = new[] { new { request_range = range } } });
            if (!TryGeneration(response.GetProperty("header").GetProperty("revision").GetString(), out var current))
                throw new InvalidDataException("Invalid namespace revision.");
            revision ??= current;
            var responses = response.GetProperty("responses");
            if (current < revision || !response.GetProperty("succeeded").GetBoolean() || responses.GetArrayLength() != 1)
                throw new InvalidDataException("Invalid namespace snapshot.");
            var page = responses[0].GetProperty("response_range");
            var more = page.TryGetProperty("more", out var continuation) && continuation.GetBoolean();
            var values = page.TryGetProperty("kvs", out var kvs) ? kvs.EnumerateArray().ToArray() : Array.Empty<JsonElement>();
            ulong count = 0;
            if (page.TryGetProperty("count", out var countValue))
            {
                var text = countValue.GetString();
                if (text != "0" && !TryGeneration(text, out count)) throw new InvalidDataException("Invalid namespace count.");
            }
            if (values.Length > limit || (more && (values.Length == 0 || count <= (ulong)values.Length)) || (!more && count != (ulong)values.Length))
                throw new InvalidDataException("Incomplete namespace page.");
            foreach (var value in values)
            {
                var key = NamespaceDecode(value.GetProperty("key"));
                if (!key.StartsWith(_prefix, StringComparison.Ordinal)) throw new InvalidDataException("Foreign namespace key.");
                var name = key[_prefix.Length..]; ClusterEngine<uint>.ValidateKeys(new[] { name });
                if (string.CompareOrdinal(key, previous) <= 0) throw new InvalidDataException("Unordered namespace keys.");
                if (!TryGeneration(value.GetProperty("mod_revision").GetString(), out var generation) || generation > revision)
                    throw new InvalidDataException("Invalid namespace generation.");
                string[] records;
                if (options.KeysOnly)
                {
                    if (value.TryGetProperty("value", out var payload) && payload.GetString() != "") throw new InvalidDataException("Unexpected namespace payload.");
                    records = Array.Empty<string>();
                }
                else
                {
                    records = JsonSerializer.Deserialize<string[]>(NamespaceDecode(value.GetProperty("value"))) ?? throw new InvalidDataException("Missing namespace journal.");
                    if (records.Any(record => record is null)) throw new InvalidDataException("Invalid namespace journal.");
                }
                var size = checked((long)NamespaceCodec.Encoding.GetByteCount(name) + 8);
                foreach (var record in records) size = checked(size + NamespaceCodec.Encoding.GetByteCount(record) + 1);
                bytes = checked(bytes + size);
                if (bytes > options.MaxBytes || entries.Count == options.MaxKeys) throw new InvalidDataException("Namespace scan limit.");
                entries.Add(new(name, new(generation, records))); previous = key;
            }
            if (!more) return entries;
            if (entries.Count == options.MaxKeys) throw new InvalidDataException("Namespace key limit.");
            start = previous + "\0";
        }
    }
    public bool CompareDelete(string key, ulong expectedGeneration)
    {
        ClusterEngine<uint>.ValidateKeys(new[] { key });
        if (expectedGeneration is 0 or > long.MaxValue) throw new ArgumentOutOfRangeException(nameof(expectedGeneration));
        var response = Post(new
        {
            compare = new[] { new { key = Key(key), target = "MOD", result = "EQUAL", mod_revision = expectedGeneration.ToString(CultureInfo.InvariantCulture) } },
            success = new[] { new { request_delete_range = new { key = Key(key) } } }
        });
        if (!TryGeneration(response.GetProperty("header").GetProperty("revision").GetString(), out _)) throw new InvalidDataException("Indeterminate deletion response.");
        var succeeded = response.TryGetProperty("succeeded", out var success) && success.GetBoolean();
        var responses = response.TryGetProperty("responses", out var results) ? results.EnumerateArray().ToArray() : Array.Empty<JsonElement>();
        if ((!succeeded && responses.Length != 0) || (succeeded && (responses.Length != 1 || responses[0].GetProperty("response_delete_range").GetProperty("deleted").GetString() != "1")))
            throw new InvalidDataException("Indeterminate deletion response.");
        return succeeded;
    }
}
