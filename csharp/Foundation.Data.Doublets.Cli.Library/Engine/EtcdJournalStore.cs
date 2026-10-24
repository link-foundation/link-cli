using System.Globalization;
using System.Net.Http;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>etcd JSON gateway. Requires a trusted durable deployment. Mutations
/// are never retried after transport errors. An injected HttpClient is caller-owned.</summary>
public sealed class EtcdJournalStore : IAtomicJournalStore, IDisposable
{
    private readonly Uri[] _endpoints;
    private readonly string _prefix;
    private readonly HttpClient _client;
    private readonly bool _owned;
    private int _cursor;
    public int MaxRequestBytes { get; set; } = 1_048_576;
    public int MaxResponseBytes { get; set; } = 8_388_608;
    public bool SupportsDurableFlush => true;
    public TimeSpan RequestTimeout { get; set; } = TimeSpan.FromSeconds(10);
    public EtcdJournalStore(IEnumerable<Uri> endpoints, string prefix = "/link-cli/engine1/", HttpClient? client = null)
    {
        _endpoints = endpoints.ToArray();
        if (_endpoints.Length == 0 || _endpoints.Any(uri => !uri.IsAbsoluteUri || uri.Scheme is not ("http" or "https") || uri.Query.Length != 0 || uri.Fragment.Length != 0))
            throw new ArgumentException("Provide HTTP(S) etcd endpoints.", nameof(endpoints));
        if (prefix.Length < 2 || !prefix.EndsWith('/')) throw new ArgumentException("Provide a nonempty prefix ending in /.", nameof(prefix));
        _prefix = prefix; _owned = client is null;
        _client = client ?? CreateClient();
    }
    [System.Diagnostics.CodeAnalysis.SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
        Justification = "The returned HttpClient owns the handler via disposeHandler:true; failure disposes it locally.")]
    private static HttpClient CreateClient()
    {
        var handler = new HttpClientHandler { AllowAutoRedirect = false };
        try { return new HttpClient(handler, disposeHandler: true) { Timeout = TimeSpan.FromSeconds(10) }; }
        catch { handler.Dispose(); throw; }
    }
    private static bool TryGeneration(string? value, out ulong generation)
    {
        generation = 0;
        return value is { Length: > 0 } && value[0] != '0' &&
            ulong.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out generation) && generation <= long.MaxValue;
    }
    private string Key(string key) => Convert.ToBase64String(Encoding.UTF8.GetBytes(_prefix + key));
    private JsonElement Post(object body)
    {
        if (RequestTimeout <= TimeSpan.Zero || RequestTimeout == Timeout.InfiniteTimeSpan || MaxRequestBytes <= 0 || MaxResponseBytes <= 0)
            throw new ArgumentOutOfRangeException(nameof(RequestTimeout), "Deadlines and byte limits must be positive and finite.");
        using var deadline = new CancellationTokenSource(RequestTimeout);
        var bytes = JsonSerializer.SerializeToUtf8Bytes(body);
        if (bytes.Length > MaxRequestBytes) throw new IOException("Coordinator request exceeds byte limit.");
        var endpoint = new Uri(_endpoints[_cursor].AbsoluteUri.TrimEnd('/') + "/v3/kv/txn");
        using var request = new HttpRequestMessage(HttpMethod.Post, endpoint) { Content = new ByteArrayContent(bytes) };
        request.Content.Headers.ContentType = new("application/json");
        using var response = _client.Send(request, HttpCompletionOption.ResponseHeadersRead, deadline.Token);
        response.EnsureSuccessStatusCode();
        using var stream = response.Content.ReadAsStream(); using var buffer = new MemoryStream();
        var chunk = new byte[8192]; int length;
        while ((length = stream.ReadAsync(chunk.AsMemory(), deadline.Token).AsTask().GetAwaiter().GetResult()) != 0)
        {
            if (buffer.Length + length > MaxResponseBytes) throw new IOException("Coordinator response exceeds byte limit.");
            buffer.Write(chunk, 0, length);
        }
        using var document = JsonDocument.Parse(buffer.ToArray()); return document.RootElement.Clone();
    }
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
    {
        ClusterEngine<uint>.ValidateKeys(keys);
        Exception? failure = null;
        for (var attempt = 0; attempt < _endpoints.Length; attempt++)
        {
            try
            {
                var result = Post(new { success = keys.Select(key => new { request_range = new { key = Key(key) } }).ToArray() });
                if (!result.GetProperty("succeeded").GetBoolean()) throw new InvalidDataException("Invalid coordinator snapshot.");
                var responses = result.GetProperty("responses").EnumerateArray().ToArray();
                if (responses.Length != keys.Count) throw new InvalidDataException("Incomplete coordinator snapshot.");
                return responses.Select((response, index) =>
                {
                    var range = response.GetProperty("response_range");
                    if (range.TryGetProperty("more", out var more) && more.GetBoolean()) throw new InvalidDataException("Incomplete coordinator range.");
                    if (!range.TryGetProperty("kvs", out var kvs) || kvs.GetArrayLength() == 0) return new JournalVersion(0, Array.Empty<string>());
                    if (kvs.GetArrayLength() != 1) throw new InvalidDataException("Invalid coordinator keys.");
                    var value = kvs[0];
                    if (value.GetProperty("key").GetString() != Key(keys[index]) ||
                        !TryGeneration(value.GetProperty("mod_revision").GetString(), out var generation))
                        throw new InvalidDataException("Invalid coordinator key or generation.");
                    var encoded = value.GetProperty("value").GetString()!;
                    var payload = Convert.FromBase64String(encoded);
                    if (Convert.ToBase64String(payload) != encoded) throw new InvalidDataException("Noncanonical coordinator base64.");
                    var entries = JsonSerializer.Deserialize<string[]>(payload)
                        ?? throw new InvalidDataException("Missing coordinator journal.");
                    if (entries.Any(entry => entry is null)) throw new InvalidDataException("Invalid coordinator journal.");
                    return new JournalVersion(generation, entries);
                }).ToArray();
            }
            catch (Exception cause) { failure = cause; _cursor = (_cursor + 1) % _endpoints.Length; }
        }
        throw new IOException("Coordinator snapshot failed.", failure);
    }
    public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
    {
        ClusterEngine<uint>.ValidateKeys(keys);
        if (expected.Count != keys.Count || updates.Count != keys.Count || expected.Any(item => item.Generation > long.MaxValue))
            throw new ArgumentException("Invalid coordinator compare arrays.");
        var compare = keys.Select((key, index) => new { key = Key(key), target = "MOD", result = "EQUAL", mod_revision = expected[index].Generation.ToString(CultureInfo.InvariantCulture) }).ToArray();
        var success = keys.Select((key, index) => (key, entries: updates[index])).Where(item => item.entries is not null)
            .Select(item => new { request_put = new { key = Key(item.key), value = Convert.ToBase64String(JsonSerializer.SerializeToUtf8Bytes(item.entries)) } }).ToArray();
        // Send exactly once, including when the peer loses the response after commit.
        var response = Post(new { compare, success });
        if (!TryGeneration(response.GetProperty("header").GetProperty("revision").GetString(), out _))
            throw new InvalidDataException("Indeterminate coordinator response.");
        // ProtoJSON omits the default false field on a rejected comparison.
        return response.TryGetProperty("succeeded", out var succeeded) && succeeded.GetBoolean();
    }
    public void Dispose() { if (_owned) _client.Dispose(); }
}
