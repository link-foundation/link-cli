using BenchmarkDotNet.Attributes;
using BenchmarkDotNet.Order;
using Foundation.Data.Doublets.Cli.Benchmarks.Models;
using Foundation.Data.Doublets.Cli.Benchmarks.Services;
using Foundation.Data.Doublets.Cli.Benchmarks.Serialization;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Benchmarks.Benchmarks;

[MemoryDiagnoser]
[Orderer(SummaryOrderPolicy.FastestToSlowest)]
[RankColumn]
public class TransportProtocolBenchmarks
{
    private LinksService _linksService = null!;
    private LinkData _testLink = null!;
    private CreateLinkRequest _createRequest = null!;
    private QueryLinksRequest _queryRequest = null!;
    
    private string _linoSerialized = null!;
    private string _jsonSerialized = null!;
    
    [GlobalSetup]
    public void Setup()
    {
        _linksService = new LinksService("benchmark.links");
        
        // Create test data
        _createRequest = new CreateLinkRequest { Source = 1, Target = 2 };
        _testLink = new LinkData { Id = 1, Source = 1, Target = 2 };
        _queryRequest = new QueryLinksRequest { Id = 1 };
        
        // Pre-serialize data for serialization benchmarks
        _linoSerialized = LinoSerializer.SerializeLinkData(_testLink);
        _jsonSerialized = JsonSerializer.Serialize(_testLink);
        
        Console.WriteLine($"LINO serialized: {_linoSerialized}");
        Console.WriteLine($"JSON serialized: {_jsonSerialized}");
    }
    
    [GlobalCleanup]
    public void Cleanup()
    {
        _linksService?.Dispose();
        
        // Clean up database files
        if (File.Exists("benchmark.links"))
            File.Delete("benchmark.links");
        if (File.Exists("benchmark.names.links"))
            File.Delete("benchmark.names.links");
    }

    #region Serialization Benchmarks
    
    [Benchmark(Description = "LINO Serialization")]
    public string LinoSerialization()
    {
        return LinoSerializer.SerializeLinkData(_testLink);
    }
    
    [Benchmark(Description = "JSON Serialization")]
    public string JsonSerialization()
    {
        return JsonSerializer.Serialize(_testLink);
    }
    
    [Benchmark(Description = "LINO Deserialization")]
    public LinkData? LinoDeserialization()
    {
        return LinoSerializer.DeserializeLinkData(_linoSerialized);
    }
    
    [Benchmark(Description = "JSON Deserialization")]
    public LinkData? JsonDeserialization()
    {
        return JsonSerializer.Deserialize<LinkData>(_jsonSerialized);
    }
    
    #endregion
    
    #region Data Operations Benchmarks
    
    [Benchmark(Description = "Create Link")]
    public async Task<LinkData> CreateLink()
    {
        var request = new CreateLinkRequest { Source = (uint)Random.Shared.Next(1000, 9999), Target = (uint)Random.Shared.Next(1000, 9999) };
        return await _linksService.CreateLinkAsync(request);
    }
    
    [Benchmark(Description = "Query Links")]
    public async Task<List<LinkData>> QueryLinks()
    {
        var request = new QueryLinksRequest { Source = 1 };
        var results = await _linksService.QueryLinksAsync(request);
        return results.ToList();
    }
    
    #endregion
    
    #region Protocol Simulation Benchmarks
    
    [Benchmark(Description = "REST-like LINO Processing")]
    public string RestLikeProcessing()
    {
        // Simulate REST API processing with LINO serialization
        var createRequestLino = LinoSerializer.SerializeCreateRequest(_createRequest);
        var parsed = ParseCreateRequestLino(createRequestLino);
        return LinoSerializer.SerializeLinkData(new LinkData { Id = 999, Source = parsed.Source, Target = parsed.Target });
    }
    
    [Benchmark(Description = "gRPC-like LINO Processing")]
    public string GrpcLikeProcessing()
    {
        // Simulate gRPC processing with LINO in message wrapper
        var wrapper = new { lino_data = LinoSerializer.SerializeLinkData(_testLink) };
        var serialized = JsonSerializer.Serialize(wrapper);
        var deserialized = JsonSerializer.Deserialize<dynamic>(serialized);
        return deserialized?.lino_data ?? "";
    }
    
    [Benchmark(Description = "GraphQL-like LINO Processing")]
    public string GraphqlLikeProcessing()
    {
        // Simulate GraphQL query resolution with LINO
        var query = $"query {{ link(id: {_testLink.Id}) {{ id source target }} }}";
        var result = LinoSerializer.SerializeLinkData(_testLink);
        return $"{{ \"data\": {{ \"link\": \"{result}\" }} }}";
    }
    
    #endregion
    
    private CreateLinkRequest ParseCreateRequestLino(string linoString)
    {
        // Simple parser for LINO create format: () ((source target))
        var parts = linoString.Replace("()", "").Replace("((", "").Replace("))", "").Trim().Split(' ');
        return new CreateLinkRequest
        {
            Source = uint.Parse(parts[0]),
            Target = uint.Parse(parts[1])
        };
    }
}