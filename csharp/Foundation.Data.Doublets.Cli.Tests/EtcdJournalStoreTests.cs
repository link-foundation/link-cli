using System.Net;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EtcdJournalStoreTests
{
    private sealed class Handler(Func<HttpRequestMessage, HttpResponseMessage> action) : HttpMessageHandler
    {
        protected override HttpResponseMessage Send(HttpRequestMessage request, CancellationToken cancellationToken) => action(request);
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken) => Task.FromResult(action(request));
    }
    private static HttpResponseMessage Response(object value) => new(HttpStatusCode.OK) { Content = new StringContent(JsonSerializer.Serialize(value)) };
    private static EtcdJournalStore Store(HttpClient client) => new(new[] { new Uri("http://one:1"), new Uri("http://two:2") }, "/test/", client);
    private static string Base64(string value) => Convert.ToBase64String(Encoding.UTF8.GetBytes(value));
    private static readonly string[] Keys = { "a", "b" };
    private static readonly JournalVersion[] Expected = { new(7, new[] { "record" }), new(0, Array.Empty<string>()) };
    private static readonly IReadOnlyList<string>?[] Updates = { new[] { "record", "new" }, null };
    private sealed class StalledBody : MemoryStream
    {
        public override async ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken = default)
        {
            await Task.Delay(TimeSpan.FromSeconds(3), cancellationToken);
            throw new IOException("The request deadline did not cancel the response body.");
        }
    }

    [Fact]
    public void OneSnapshotAndCasCompareEveryShardAndOmittedFalseMeansConflict()
    {
        var calls = new List<JsonElement>();
        using var handler = new Handler(request =>
        {
            using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            calls.Add(body.RootElement.Clone());
            return calls.Count == 1 ? Response(new
            {
                succeeded = true,
                responses = new object[] {
                new { response_range = new { kvs = new[] { new { key = Base64("/test/a"), mod_revision = "7", value = Base64("[\"record\"]") } } } },
                new { response_range = new { } } }
            }) : Response(new { header = new { revision = "9" } });
        });
        using var client = new HttpClient(handler, disposeHandler: false);
        using var store = Store(client);
        var read = store.Read(Keys); Assert.Equal(Expected[0].Entries, read[0].Entries); Assert.Equal(0UL, read[1].Generation);
        Assert.False(store.CompareExchange(Keys, read, Updates));
        Assert.Equal(2, calls[0].GetProperty("success").GetArrayLength());
        Assert.Equal("MOD", calls[1].GetProperty("compare")[1].GetProperty("target").GetString());
        Assert.Equal("0", calls[1].GetProperty("compare")[1].GetProperty("mod_revision").GetString());
        Assert.Equal(1, calls[1].GetProperty("success").GetArrayLength());
    }

    [Fact]
    public void ReadsFailOverButMutationsAreSentExactlyOnceAndInjectedClientRemainsOwned()
    {
        var count = 0;
        using var handler = new Handler(request =>
        {
            count++;
            if (request.RequestUri!.Host == "one" || count == 3) throw new HttpRequestException("lost response");
            return Response(new { succeeded = true, responses = new[] { new { response_range = new { } } } });
        });
        using var client = new HttpClient(handler, disposeHandler: false);
        var store = Store(client); var snapshot = store.Read(new[] { "a" }); Assert.Equal(2, count);
        Assert.Throws<HttpRequestException>(() => store.CompareExchange(new[] { "a" }, snapshot, new IReadOnlyList<string>?[] { new[] { "new" } }));
        Assert.Equal(3, count); store.Dispose();
        using var request = new HttpRequestMessage(HttpMethod.Post, "http://two:2");
        using var response = client.Send(request); Assert.Equal(4, count);
    }

    [Theory]
    [InlineData("+1")]
    [InlineData("01")]
    [InlineData("0")]
    [InlineData("9223372036854775808")]
    public void InvalidGenerationsAreRejected(string generation)
    {
        using var handler = new Handler(_ => Response(new
        {
            succeeded = true,
            responses = new[] {
            new { response_range = new { kvs = new[] { new { key = Base64("/test/a"), mod_revision = generation, value = Base64("[]") } } } } }
        }));
        using var client = new HttpClient(handler, disposeHandler: false);
        using var store = Store(client); Assert.Throws<IOException>(() => store.Read(new[] { "a" }));
    }

    [Fact]
    public void InvalidRepliesAndLimitsFailClosedBeforePublishing()
    {
        var count = 0;
        using var handler = new Handler(_ => { count++; return Response(new { }); });
        using var client = new HttpClient(handler, disposeHandler: false);
        using var store = Store(client);
        Assert.Throws<KeyNotFoundException>(() => store.CompareExchange(Keys, Expected, Updates)); Assert.Equal(1, count);
        store.MaxRequestBytes = 1; Assert.Throws<IOException>(() => store.CompareExchange(Keys, Expected, Updates)); Assert.Equal(1, count);
        store.MaxRequestBytes = 10000; store.MaxResponseBytes = 1;
        Assert.Throws<IOException>(() => store.Read(Keys)); Assert.Equal(3, count);
        Assert.Throws<ArgumentException>(() => store.Read(new[] { "a", "a" }));
        Assert.Throws<ArgumentException>(() => new EtcdJournalStore(Array.Empty<Uri>()));
        Assert.Throws<ArgumentException>(() => new EtcdJournalStore(new[] { new Uri("ftp://localhost") }));
    }

    [Fact]
    public void MalformedGatewaySnapshotsRejectPartialRangesWrongKeysAndInvalidPayloads()
    {
        object Snapshot(object range) => new { succeeded = true, responses = new[] { new { response_range = range } } };
        object Value(string payload, string? key = null) => new { key = key ?? Base64("/test/a"), mod_revision = "1", value = payload };
        var value = Value(Base64("[]"));
        foreach (var reply in new object[]
        {
            new { succeeded = false },
            new { succeeded = true, responses = Array.Empty<object>() },
            Snapshot(new { more = true }),
            Snapshot(new { kvs = new[] { value, value } }),
            Snapshot(new { kvs = new[] { Value(Base64("[]"), Base64("/wrong/a")) } }),
            Snapshot(new { kvs = new[] { Value("W10=\n") } }),
            Snapshot(new { kvs = new[] { Value("!") } }),
            Snapshot(new { kvs = new[] { Value(Base64("null")) } }),
            Snapshot(new { kvs = new[] { Value(Base64("[null]")) } }),
            Snapshot(new { kvs = new[] { Value(Base64("[7]")) } }),
            Snapshot(new { kvs = new[] { Value(Base64("[")) } })
        })
        {
            var calls = 0;
            using var handler = new Handler(_ => { calls++; return Response(reply); });
            using var client = new HttpClient(handler, disposeHandler: false);
            using var store = Store(client);
            var error = Assert.Throws<IOException>(() => store.Read(new[] { "a" }));
            Assert.NotNull(error.InnerException); Assert.Equal(2, calls);
        }
    }

    [Fact]
    public void SuccessfulPublicationAndEmptyRangeAreDistinctFromInvalidCommitHeaders()
    {
        using var handler = new Handler(_ => Response(new { header = new { revision = "2" }, succeeded = true }));
        using var client = new HttpClient(handler, disposeHandler: false);
        using var store = Store(client);
        Assert.True(store.CompareExchange(Keys, Expected, Updates));
        Assert.True(store.SupportsDurableFlush);
        foreach (var generation in new string?[] { null, "", "0", "01", "-1", "9223372036854775808" })
        {
            var calls = 0;
            using var invalidHandler = new Handler(_ => { calls++; return Response(new { header = new { revision = generation } }); });
            using var invalidClient = new HttpClient(invalidHandler, disposeHandler: false);
            using var invalid = Store(invalidClient);
            Assert.Throws<InvalidDataException>(() => invalid.CompareExchange(Keys, Expected, Updates));
            Assert.Equal(1, calls);
        }
        using var emptyHandler = new Handler(_ => Response(new { succeeded = true, responses = new[] { new { response_range = new { more = false, kvs = Array.Empty<object>() } } } }));
        using var emptyClient = new HttpClient(emptyHandler, disposeHandler: false);
        using var empty = Store(emptyClient); Assert.Equal(0UL, empty.Read(new[] { "a" })[0].Generation);
    }

    [Fact]
    public void InvalidConfigurationAndCompareCoordinatesNeverSendARequest()
    {
        var calls = 0;
        using var handler = new Handler(_ => { calls++; return Response(new { }); });
        using var client = new HttpClient(handler, disposeHandler: false);
        using var store = Store(client);
        foreach (var endpoint in new[] { new Uri("relative", UriKind.Relative), new Uri("http://localhost/?query"), new Uri("http://localhost/#fragment") })
            Assert.Throws<ArgumentException>(() => new EtcdJournalStore(new[] { endpoint }, client: client));
        foreach (var prefix in new[] { "", "/", "/no-terminator" })
            Assert.Throws<ArgumentException>(() => new EtcdJournalStore(new[] { new Uri("https://localhost/") }, prefix, client));
        using (var owned = new EtcdJournalStore(new[] { new Uri("https://localhost/") })) Assert.True(owned.SupportsDurableFlush);
        Assert.Throws<ArgumentException>(() => store.CompareExchange(Keys, Array.Empty<JournalVersion>(), Updates));
        Assert.Throws<ArgumentException>(() => store.CompareExchange(Keys, Expected, Array.Empty<IReadOnlyList<string>?>()));
        Assert.Throws<ArgumentException>(() => store.CompareExchange(new[] { "a" }, new[] { new JournalVersion(ulong.MaxValue, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { null }));
        foreach (var deadline in new[] { TimeSpan.Zero, TimeSpan.FromSeconds(-2), Timeout.InfiniteTimeSpan })
        { store.RequestTimeout = deadline; Assert.Throws<ArgumentOutOfRangeException>(() => store.CompareExchange(Keys, Expected, Updates)); }
        store.RequestTimeout = TimeSpan.FromSeconds(1);
        store.MaxRequestBytes = 0; Assert.Throws<ArgumentOutOfRangeException>(() => store.CompareExchange(Keys, Expected, Updates));
        store.MaxRequestBytes = 10000; store.MaxResponseBytes = 0; Assert.Throws<ArgumentOutOfRangeException>(() => store.CompareExchange(Keys, Expected, Updates));
        Assert.Equal(0, calls);
    }

    [Fact]
    public void CommitDeadlineIncludesAStalledResponseBodyAndDoesNotRetry()
    {
        var calls = 0;
        using var handler = new Handler(_ =>
        { calls++; return new(HttpStatusCode.OK) { Content = new StreamContent(new StalledBody()) }; });
        using var client = new HttpClient(handler, disposeHandler: false);
        using var store = Store(client); store.RequestTimeout = TimeSpan.FromMilliseconds(50);
        Assert.ThrowsAny<OperationCanceledException>(() => store.CompareExchange(Keys, Expected, Updates));
        Assert.Equal(1, calls);
    }
}
