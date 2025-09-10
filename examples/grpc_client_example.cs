// Example GRPC client usage for LINO API
// This demonstrates how to use the LINO GRPC API instead of JSON

using Grpc.Net.Client;
using Foundation.Data.Doublets.Cli.Grpc;

// Example usage of the LINO GRPC API
public class LinoGrpcClientExample
{
    private readonly LinoService.LinoServiceClient _client;

    public LinoGrpcClientExample(string serverAddress = "https://localhost:5001")
    {
        var channel = GrpcChannel.ForAddress(serverAddress);
        _client = new LinoService.LinoServiceClient(channel);
    }

    // Example 1: Create links using LINO notation
    public async Task CreateLinksExample()
    {
        var request = new LinoQueryRequest
        {
            Query = "() ((1 1) (2 2))",  // Create links (1 1) and (2 2)
            IncludeChanges = true,
            IncludeAfterState = true
        };

        var response = await _client.ExecuteQueryAsync(request);
        
        Console.WriteLine("LINO Create Operation:");
        Console.WriteLine($"Success: {response.Success}");
        
        if (response.Changes.Any())
        {
            Console.WriteLine("Changes:");
            foreach (var change in response.Changes)
            {
                Console.WriteLine($"  {change}");
            }
        }

        if (response.AfterState.Any())
        {
            Console.WriteLine("After State:");
            foreach (var link in response.AfterState)
            {
                Console.WriteLine($"  {link}");
            }
        }
    }

    // Example 2: Update links using LINO notation  
    public async Task UpdateLinksExample()
    {
        var request = new LinoQueryRequest
        {
            Query = "((1: 1 1)) ((1: 1 2))",  // Update link 1 to point from 1 to 2
            IncludeChanges = true
        };

        var response = await _client.ExecuteQueryAsync(request);
        
        Console.WriteLine("LINO Update Operation:");
        Console.WriteLine($"Success: {response.Success}");
        
        foreach (var change in response.Changes)
        {
            Console.WriteLine($"Change: {change}");
        }
    }

    // Example 3: Read all links
    public async Task ReadAllLinksExample()
    {
        var request = new GetAllLinksRequest
        {
            IncludeNames = true
        };

        var response = await _client.GetAllLinksAsync(request);
        
        Console.WriteLine("All Links:");
        foreach (var link in response.Links)
        {
            Console.WriteLine($"  {link}");
        }
    }

    // Example 4: Batch operations
    public async Task BatchOperationsExample()
    {
        var batchRequest = new LinoBatchRequest
        {
            StopOnError = true
        };

        // Add multiple LINO queries
        batchRequest.Queries.Add(new LinoQueryRequest 
        { 
            Query = "() ((3 3))",  // Create link (3 3)
            IncludeChanges = true 
        });
        
        batchRequest.Queries.Add(new LinoQueryRequest 
        { 
            Query = "() ((4 4))",  // Create link (4 4)
            IncludeChanges = true 
        });

        var response = await _client.ExecuteBatchAsync(batchRequest);
        
        Console.WriteLine("Batch Operation:");
        Console.WriteLine($"Success: {response.Success}");
        Console.WriteLine($"Successful operations: {response.SuccessfulOperations}");
        Console.WriteLine($"Failed operations: {response.FailedOperations}");
    }

    // Example 5: Streaming operations
    public async Task StreamingExample()
    {
        using var stream = _client.StreamQueries();

        // Send multiple queries through the stream
        await stream.RequestStream.WriteAsync(new LinoStreamRequest
        {
            Query = new LinoQueryRequest
            {
                Query = "() ((5 5))",
                IncludeChanges = true
            }
        });

        await stream.RequestStream.WriteAsync(new LinoStreamRequest
        {
            Query = new LinoQueryRequest
            {
                Query = "() ((6 6))",
                IncludeChanges = true
            }
        });

        await stream.RequestStream.CompleteAsync();

        // Read responses
        await foreach (var response in stream.ResponseStream.ReadAllAsync())
        {
            if (response.QueryResponse != null)
            {
                Console.WriteLine($"Stream Response Success: {response.QueryResponse.Success}");
                foreach (var change in response.QueryResponse.Changes)
                {
                    Console.WriteLine($"  Change: {change}");
                }
            }
        }
    }
}

// Main usage example
class Program
{
    static async Task Main(string[] args)
    {
        var client = new LinoGrpcClientExample();
        
        try
        {
            Console.WriteLine("=== LINO GRPC API Examples ===");
            
            await client.CreateLinksExample();
            await client.UpdateLinksExample();
            await client.ReadAllLinksExample();
            await client.BatchOperationsExample();
            await client.StreamingExample();
        }
        catch (Exception ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}