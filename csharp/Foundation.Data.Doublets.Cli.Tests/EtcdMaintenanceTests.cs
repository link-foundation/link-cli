using System.Net;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class EtcdMaintenanceTests
{
    private sealed class Handler(Func<HttpRequestMessage, HttpResponseMessage> action) : HttpMessageHandler
    {
        protected override HttpResponseMessage Send(HttpRequestMessage request, CancellationToken cancellationToken) => action(request);
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken) => Task.FromResult(action(request));
    }
    private static JsonElement Vectors()
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "journal-maintenance-v1.json")));
        return document.RootElement.Clone();
    }
    private static HttpResponseMessage Reply(JsonElement value) => new(HttpStatusCode.OK) { Content = new StringContent(value.GetRawText()) };
    private static JournalScanOptions Options(JsonElement value) => new(
        value.TryGetProperty("maxKeys", out var keys) ? keys.GetInt32() : 4096,
        value.TryGetProperty("maxBytes", out var bytes) ? bytes.GetInt64() : 8_388_608,
        value.TryGetProperty("pageKeys", out var page) ? page.GetInt32() : 128,
        value.TryGetProperty("keysOnly", out var only) && only.GetBoolean());
    [Fact(Timeout = 15000)]
    public async Task SharedScanVectors()
    {
        await Task.Yield();
        foreach (var vector in Vectors().GetProperty("scans").EnumerateArray())
        {
            var replies = vector.GetProperty("replies"); var calls = new List<JsonElement>();
            using var handler = new Handler(request =>
            {
                using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult()); calls.Add(body.RootElement.Clone());
                Assert.True(calls.Count <= replies.GetArrayLength(), vector.GetProperty("name").GetString()); return Reply(replies[calls.Count - 1]);
            });
            using var client = new HttpClient(handler, disposeHandler: false);
            using var store = new EtcdJournalStore(new[] { new Uri("http://one:1") }, vector.GetProperty("prefix").GetString()!, client);
            var expected = vector.GetProperty("expected");
            if (expected.ValueKind == JsonValueKind.Null) Assert.Throws<IOException>(() => store.ScanNamespace(Options(vector.GetProperty("options"))));
            else
            {
                var actual = store.ScanNamespace(Options(vector.GetProperty("options")));
                Assert.Equal(expected.GetArrayLength(), actual.Count);
                for (var index = 0; index < actual.Count; index++)
                {
                    Assert.Equal(expected[index].GetProperty("key").GetString(), actual[index].Key);
                    Assert.Equal(ulong.Parse(expected[index].GetProperty("version").GetProperty("generation").GetString()!, System.Globalization.CultureInfo.InvariantCulture), actual[index].Version.Generation);
                    Assert.Equal(expected[index].GetProperty("version").GetProperty("entries").EnumerateArray().Select(value => value.GetString()), actual[index].Version.Entries);
                }
            }
            Assert.Equal(replies.GetArrayLength(), calls.Count);
            Assert.False(calls[0].GetProperty("success")[0].GetProperty("request_range").TryGetProperty("revision", out _));
            if (calls.Count > 1) Assert.Equal("10", calls[1].GetProperty("success")[0].GetProperty("request_range").GetProperty("revision").GetString());
        }
    }
    [Fact(Timeout = 15000)]
    public async Task SharedDeleteVectors()
    {
        await Task.Yield();
        foreach (var vector in Vectors().GetProperty("deletes").EnumerateArray())
        {
            var count = 0;
            using var handler = new Handler(request =>
            {
                count++; using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult()); var root = body.RootElement;
                Assert.Equal("MOD", root.GetProperty("compare")[0].GetProperty("target").GetString());
                Assert.Equal("7", root.GetProperty("compare")[0].GetProperty("mod_revision").GetString());
                var deletion = root.GetProperty("success")[0].GetProperty("request_delete_range");
                Assert.Equal(Convert.ToBase64String(Encoding.UTF8.GetBytes("/test/a")), deletion.GetProperty("key").GetString());
                Assert.False(deletion.TryGetProperty("range_end", out _)); return Reply(vector.GetProperty("reply"));
            });
            using var client = new HttpClient(handler, disposeHandler: false);
            using var store = new EtcdJournalStore(new[] { new Uri("http://one:1"), new Uri("http://two:2") }, "/test/", client);
            var expected = vector.GetProperty("expected");
            if (expected.ValueKind == JsonValueKind.Null) Assert.NotNull(Record.Exception(() => store.CompareDelete("a", 7)));
            else Assert.Equal(expected.GetBoolean(), store.CompareDelete("a", 7));
            Assert.Equal(1, count);
        }
    }
    [Fact(Timeout = 5000)]
    public async Task OptionsFailBeforeIo()
    {
        await Task.Yield();
        using var store = new EtcdJournalStore(new[] { new Uri("http://127.0.0.1:1") }, "/test/"); IJournalMaintenanceStore maintenance = store;
        foreach (var options in new[] { new JournalScanOptions(MaxKeys: 0), new JournalScanOptions(MaxBytes: 0), new JournalScanOptions(PageKeys: 0), new JournalScanOptions(PageKeys: 257) })
            Assert.Throws<ArgumentOutOfRangeException>(() => maintenance.ScanNamespace(options));
        foreach (var generation in new[] { 0UL, (ulong)long.MaxValue + 1 }) Assert.Throws<ArgumentOutOfRangeException>(() => maintenance.CompareDelete("a", generation));
        foreach (var key in new[] { "", "/", "a/b" }) Assert.Throws<ArgumentException>(() => maintenance.CompareDelete(key, 7));
    }
    [Fact(Timeout = 5000)]
    public async Task ReadFailoverRestartsAndLostDeleteIsNeverRetried()
    {
        await Task.Yield();
        var count = 0;
        using var handler = new Handler(request =>
        {
            count++;
            if (count is 2 or 4) throw new HttpRequestException("lost response");
            using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            if (count == 1) return Reply(Vectors().GetProperty("scans").EnumerateArray().Single(value => value.GetProperty("name").GetString() == "pinned pagination").GetProperty("replies")[0]);
            Assert.Equal("two", request.RequestUri!.Host);
            var range = body.RootElement.GetProperty("success")[0].GetProperty("request_range");
            Assert.False(range.TryGetProperty("revision", out _));
            Assert.Equal(Convert.ToBase64String(Encoding.UTF8.GetBytes("/test/")), range.GetProperty("key").GetString());
            return Reply(Vectors().GetProperty("scans")[0].GetProperty("replies")[0]);
        });
        using var client = new HttpClient(handler, disposeHandler: false);
        using var store = new EtcdJournalStore(new[] { new Uri("http://one:1"), new Uri("http://two:2") }, "/test/", client);
        Assert.Empty(store.ScanNamespace(new(PageKeys: 2)));
        Assert.Throws<HttpRequestException>(() => store.CompareDelete("a", 7)); Assert.Equal(4, count);
    }
}
