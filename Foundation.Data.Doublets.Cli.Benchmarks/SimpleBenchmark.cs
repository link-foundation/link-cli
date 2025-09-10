using System.Diagnostics;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Benchmarks.Models;
using Foundation.Data.Doublets.Cli.Benchmarks.Services;
using Foundation.Data.Doublets.Cli.Benchmarks.Serialization;

namespace Foundation.Data.Doublets.Cli.Benchmarks;

public static class SimpleBenchmark
{
    public static async Task RunBenchmarksAsync()
    {
        Console.WriteLine("LINO API Transport Protocols Benchmark");
        Console.WriteLine("=====================================");
        Console.WriteLine();
        
        using var linksService = new LinksService("benchmark.links");
        var testLink = new LinkData { Id = 1, Source = 1, Target = 2 };
        
        // Warmup
        Console.WriteLine("Warming up...");
        for (int i = 0; i < 1000; i++)
        {
            _ = LinoSerializer.SerializeLinkData(testLink);
            _ = JsonSerializer.Serialize(testLink);
        }
        
        Console.WriteLine("Starting benchmarks...");
        Console.WriteLine();
        
        // Serialization benchmarks
        await RunSerializationBenchmarks(testLink);
        
        // Data operations benchmarks  
        await RunDataOperationsBenchmarks(linksService);
        
        // Protocol simulation benchmarks
        await RunProtocolSimulationBenchmarks(testLink);
        
        // Cleanup
        CleanupDatabaseFiles();
        
        Console.WriteLine("Benchmarks completed!");
    }
    
    private static async Task RunSerializationBenchmarks(LinkData testLink)
    {
        const int iterations = 100000;
        
        Console.WriteLine("=== Serialization Benchmarks ===");
        
        // LINO Serialization
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iterations; i++)
        {
            _ = LinoSerializer.SerializeLinkData(testLink);
        }
        sw.Stop();
        var linoSerTime = sw.ElapsedMilliseconds;
        
        // JSON Serialization
        sw.Restart();
        for (int i = 0; i < iterations; i++)
        {
            _ = JsonSerializer.Serialize(testLink);
        }
        sw.Stop();
        var jsonSerTime = sw.ElapsedMilliseconds;
        
        // Deserialization
        var linoString = LinoSerializer.SerializeLinkData(testLink);
        var jsonString = JsonSerializer.Serialize(testLink);
        
        // LINO Deserialization
        sw.Restart();
        for (int i = 0; i < iterations; i++)
        {
            _ = LinoSerializer.DeserializeLinkData(linoString);
        }
        sw.Stop();
        var linoDeserTime = sw.ElapsedMilliseconds;
        
        // JSON Deserialization
        sw.Restart();
        for (int i = 0; i < iterations; i++)
        {
            _ = JsonSerializer.Deserialize<LinkData>(jsonString);
        }
        sw.Stop();
        var jsonDeserTime = sw.ElapsedMilliseconds;
        
        Console.WriteLine($"LINO Serialization ({iterations:N0} iterations): {linoSerTime:N0} ms ({(double)iterations / linoSerTime:N0} ops/ms)");
        Console.WriteLine($"JSON Serialization ({iterations:N0} iterations): {jsonSerTime:N0} ms ({(double)iterations / jsonSerTime:N0} ops/ms)");
        Console.WriteLine($"LINO Deserialization ({iterations:N0} iterations): {linoDeserTime:N0} ms ({(double)iterations / linoDeserTime:N0} ops/ms)");  
        Console.WriteLine($"JSON Deserialization ({iterations:N0} iterations): {jsonDeserTime:N0} ms ({(double)iterations / jsonDeserTime:N0} ops/ms)");
        
        Console.WriteLine($"LINO vs JSON Serialization: {(double)jsonSerTime / linoSerTime:F2}x");
        Console.WriteLine($"LINO vs JSON Deserialization: {(double)jsonDeserTime / linoDeserTime:F2}x");
        Console.WriteLine();
    }
    
    private static async Task RunDataOperationsBenchmarks(LinksService linksService)
    {
        const int iterations = 1000;
        
        Console.WriteLine("=== Data Operations Benchmarks ===");
        
        // Create operations
        var sw = Stopwatch.StartNew();
        var createdIds = new List<uint>();
        for (int i = 0; i < iterations; i++)
        {
            var request = new CreateLinkRequest 
            { 
                Source = (uint)(i + 1000), 
                Target = (uint)(i + 2000) 
            };
            var result = await linksService.CreateLinkAsync(request);
            createdIds.Add(result.Id);
        }
        sw.Stop();
        var createTime = sw.ElapsedMilliseconds;
        
        // Query operations
        sw.Restart();
        for (int i = 0; i < iterations && i < createdIds.Count; i++)
        {
            _ = await linksService.GetLinkAsync(createdIds[i]);
        }
        sw.Stop();
        var queryTime = sw.ElapsedMilliseconds;
        
        Console.WriteLine($"Create Links ({iterations:N0} operations): {createTime:N0} ms ({(double)iterations / createTime:N0} ops/ms)");
        Console.WriteLine($"Query Links ({iterations:N0} operations): {queryTime:N0} ms ({(double)iterations / queryTime:N0} ops/ms)");
        Console.WriteLine();
    }
    
    private static async Task RunProtocolSimulationBenchmarks(LinkData testLink)
    {
        const int iterations = 10000;
        
        Console.WriteLine("=== Protocol Simulation Benchmarks ===");
        
        // REST-like LINO Processing
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iterations; i++)
        {
            var createRequest = new CreateLinkRequest { Source = testLink.Source, Target = testLink.Target };
            var createRequestLino = LinoSerializer.SerializeCreateRequest(createRequest);
            var parsed = ParseCreateRequestLino(createRequestLino);
            _ = LinoSerializer.SerializeLinkData(new LinkData { Id = 999, Source = parsed.Source, Target = parsed.Target });
        }
        sw.Stop();
        var restTime = sw.ElapsedMilliseconds;
        
        // gRPC-like LINO Processing
        sw.Restart();
        for (int i = 0; i < iterations; i++)
        {
            var wrapper = new { lino_data = LinoSerializer.SerializeLinkData(testLink) };
            var serialized = JsonSerializer.Serialize(wrapper);
            var deserialized = JsonSerializer.Deserialize<Dictionary<string, object>>(serialized);
            _ = deserialized?["lino_data"]?.ToString() ?? "";
        }
        sw.Stop();
        var grpcTime = sw.ElapsedMilliseconds;
        
        // GraphQL-like LINO Processing
        sw.Restart();
        for (int i = 0; i < iterations; i++)
        {
            var query = $"query {{ link(id: {testLink.Id}) {{ id source target }} }}";
            var result = LinoSerializer.SerializeLinkData(testLink);
            _ = $"{{ \"data\": {{ \"link\": \"{result}\" }} }}";
        }
        sw.Stop();
        var graphqlTime = sw.ElapsedMilliseconds;
        
        Console.WriteLine($"REST-like Processing ({iterations:N0} operations): {restTime:N0} ms ({(double)iterations / restTime:N0} ops/ms)");
        Console.WriteLine($"gRPC-like Processing ({iterations:N0} operations): {grpcTime:N0} ms ({(double)iterations / grpcTime:N0} ops/ms)");
        Console.WriteLine($"GraphQL-like Processing ({iterations:N0} operations): {graphqlTime:N0} ms ({(double)iterations / graphqlTime:N0} ops/ms)");
        
        // Relative performance comparison
        Console.WriteLine();
        Console.WriteLine("=== Transport Protocol Performance Comparison ===");
        var minTime = Math.Min(Math.Min(restTime, grpcTime), graphqlTime);
        Console.WriteLine($"REST-like: {(double)restTime / minTime:F2}x relative performance");
        Console.WriteLine($"gRPC-like: {(double)grpcTime / minTime:F2}x relative performance");  
        Console.WriteLine($"GraphQL-like: {(double)graphqlTime / minTime:F2}x relative performance");
        Console.WriteLine();
    }
    
    private static CreateLinkRequest ParseCreateRequestLino(string linoString)
    {
        // Simple parser for LINO create format: () ((source target))
        var parts = linoString.Replace("()", "").Replace("((", "").Replace("))", "").Trim().Split(' ');
        return new CreateLinkRequest
        {
            Source = uint.Parse(parts[0]),
            Target = uint.Parse(parts[1])
        };
    }
    
    private static void CleanupDatabaseFiles()
    {
        try
        {
            if (File.Exists("benchmark.links"))
                File.Delete("benchmark.links");
            if (File.Exists("benchmark.names.links"))
                File.Delete("benchmark.names.links");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"Warning: Could not clean up database files: {ex.Message}");
        }
    }
}