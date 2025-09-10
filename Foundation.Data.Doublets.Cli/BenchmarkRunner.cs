using System.Diagnostics;
using System.Net;

namespace Foundation.Data.Doublets.Cli
{
    public class BenchmarkRunner
    {
        private readonly string _databasePath;
        private readonly bool _trace;

        public BenchmarkRunner(string databasePath, bool trace = false)
        {
            _databasePath = databasePath ?? throw new ArgumentNullException(nameof(databasePath));
            _trace = trace;
        }

        public async Task<BenchmarkResults> RunBenchmarkAsync(BenchmarkOptions options)
        {
            Console.WriteLine("Starting benchmark: CLI access vs LiNo protocol server access");
            Console.WriteLine($"Test queries: {options.TestQueries.Count}");
            Console.WriteLine($"Iterations per query: {options.IterationsPerQuery}");
            Console.WriteLine($"Warmup iterations: {options.WarmupIterations}");
            Console.WriteLine();

            var results = new BenchmarkResults();

            // Benchmark CLI access (direct file access)
            Console.WriteLine("Benchmarking CLI access (direct file access)...");
            results.CliResults = await BenchmarkCliAccessAsync(options);

            // Benchmark server access
            Console.WriteLine("Benchmarking LiNo protocol server access...");
            results.ServerResults = await BenchmarkServerAccessAsync(options);

            return results;
        }

        private async Task<AccessMethodResults> BenchmarkCliAccessAsync(BenchmarkOptions options)
        {
            var results = new AccessMethodResults { MethodName = "CLI Direct Access" };
            var allMeasurements = new List<long>();

            foreach (var query in options.TestQueries)
            {
                var queryMeasurements = new List<long>();

                // Warmup
                for (int i = 0; i < options.WarmupIterations; i++)
                {
                    await ExecuteCliQueryAsync(query);
                }

                // Actual measurements
                for (int i = 0; i < options.IterationsPerQuery; i++)
                {
                    var stopwatch = Stopwatch.StartNew();
                    await ExecuteCliQueryAsync(query);
                    stopwatch.Stop();
                    
                    queryMeasurements.Add(stopwatch.ElapsedMilliseconds);
                    allMeasurements.Add(stopwatch.ElapsedMilliseconds);
                }

                results.QueryResults[query] = new QueryResults
                {
                    AverageLatencyMs = queryMeasurements.Average(),
                    MinLatencyMs = queryMeasurements.Min(),
                    MaxLatencyMs = queryMeasurements.Max(),
                    MedianLatencyMs = GetMedian(queryMeasurements),
                    SuccessfulOperations = queryMeasurements.Count
                };

                Console.WriteLine($"  Query: {query.Substring(0, Math.Min(50, query.Length))}...");
                Console.WriteLine($"    Avg: {results.QueryResults[query].AverageLatencyMs:F2}ms");
            }

            results.OverallAverageLatencyMs = allMeasurements.Average();
            results.OverallMinLatencyMs = allMeasurements.Min();
            results.OverallMaxLatencyMs = allMeasurements.Max();
            results.OverallMedianLatencyMs = GetMedian(allMeasurements);
            results.TotalOperations = allMeasurements.Count;
            results.SuccessfulOperations = allMeasurements.Count;
            results.FailedOperations = 0;

            return results;
        }

        private async Task<AccessMethodResults> BenchmarkServerAccessAsync(BenchmarkOptions options)
        {
            var results = new AccessMethodResults { MethodName = "LiNo Protocol Server Access" };
            var allMeasurements = new List<long>();

            using var server = await StartServerAsync(options.ServerPort);
            await Task.Delay(1000); // Give server time to start

            foreach (var query in options.TestQueries)
            {
                var queryMeasurements = new List<long>();
                var successfulOperations = 0;

                // Warmup
                for (int i = 0; i < options.WarmupIterations; i++)
                {
                    try
                    {
                        await ExecuteServerQueryAsync(query, options.ServerPort);
                    }
                    catch
                    {
                        // Ignore warmup failures
                    }
                }

                // Actual measurements
                for (int i = 0; i < options.IterationsPerQuery; i++)
                {
                    try
                    {
                        var stopwatch = Stopwatch.StartNew();
                        await ExecuteServerQueryAsync(query, options.ServerPort);
                        stopwatch.Stop();
                        
                        queryMeasurements.Add(stopwatch.ElapsedMilliseconds);
                        allMeasurements.Add(stopwatch.ElapsedMilliseconds);
                        successfulOperations++;
                    }
                    catch (Exception ex)
                    {
                        Console.WriteLine($"    Query failed: {ex.Message}");
                    }
                }

                if (queryMeasurements.Any())
                {
                    results.QueryResults[query] = new QueryResults
                    {
                        AverageLatencyMs = queryMeasurements.Average(),
                        MinLatencyMs = queryMeasurements.Min(),
                        MaxLatencyMs = queryMeasurements.Max(),
                        MedianLatencyMs = GetMedian(queryMeasurements),
                        SuccessfulOperations = successfulOperations
                    };

                    Console.WriteLine($"  Query: {query.Substring(0, Math.Min(50, query.Length))}...");
                    Console.WriteLine($"    Avg: {results.QueryResults[query].AverageLatencyMs:F2}ms");
                }
            }

            if (allMeasurements.Any())
            {
                results.OverallAverageLatencyMs = allMeasurements.Average();
                results.OverallMinLatencyMs = allMeasurements.Min();
                results.OverallMaxLatencyMs = allMeasurements.Max();
                results.OverallMedianLatencyMs = GetMedian(allMeasurements);
            }

            results.TotalOperations = options.TestQueries.Count * options.IterationsPerQuery;
            results.SuccessfulOperations = allMeasurements.Count;
            results.FailedOperations = results.TotalOperations - results.SuccessfulOperations;

            return results;
        }

        private async Task ExecuteCliQueryAsync(string query)
        {
            var links = new NamedLinksDecorator<uint>(_databasePath, _trace);
            var options = new AdvancedMixedQueryProcessor.Options { Query = query };
            
            await Task.Run(() => AdvancedMixedQueryProcessor.ProcessQuery(links, options));
            
            // Add small delay to ensure file handles are released
            await Task.Delay(10);
        }

        private async Task ExecuteServerQueryAsync(string query, int port)
        {
            using var client = new LinoProtocolClient("localhost", port);
            await client.ConnectAsync();
            var response = await client.SendQueryAsync(query);
            
            if (!response.Success)
            {
                throw new Exception($"Server query failed: {response.Error}");
            }
        }

        private Task<LinoProtocolServer> StartServerAsync(int port)
        {
            // Create a separate database path for the server to avoid file locking conflicts
            var serverDbPath = _databasePath.Replace(".links", ".server.links");
            
            // Copy the main database to the server database if it exists and server db doesn't exist
            if (File.Exists(_databasePath) && !File.Exists(serverDbPath))
            {
                File.Copy(_databasePath, serverDbPath);
                
                // Also copy the names database if it exists
                var mainNamesDbPath = _databasePath.Replace(".links", ".names.links");
                var serverNamesDbPath = serverDbPath.Replace(".links", ".names.links");
                if (File.Exists(mainNamesDbPath))
                {
                    File.Copy(mainNamesDbPath, serverNamesDbPath);
                }
            }
            
            var links = new NamedLinksDecorator<uint>(serverDbPath, _trace);
            var server = new LinoProtocolServer(links, IPAddress.Loopback, port);
            
            Task.Run(async () => await server.StartAsync());
            
            return Task.FromResult(server);
        }

        private static double GetMedian(List<long> values)
        {
            if (!values.Any()) return 0;
            
            var sorted = values.OrderBy(x => x).ToList();
            var count = sorted.Count;
            
            if (count % 2 == 0)
            {
                return (sorted[count / 2 - 1] + sorted[count / 2]) / 2.0;
            }
            else
            {
                return sorted[count / 2];
            }
        }
    }

    public class BenchmarkOptions
    {
        public List<string> TestQueries { get; set; } = new List<string>();
        public int IterationsPerQuery { get; set; } = 10;
        public int WarmupIterations { get; set; } = 3;
        public int ServerPort { get; set; } = 8080;
    }

    public class BenchmarkResults
    {
        public AccessMethodResults? CliResults { get; set; }
        public AccessMethodResults? ServerResults { get; set; }

        public void PrintReport()
        {
            Console.WriteLine();
            Console.WriteLine("=== BENCHMARK RESULTS ===");
            Console.WriteLine();

            if (CliResults != null)
            {
                PrintAccessMethodResults(CliResults);
            }

            if (ServerResults != null)
            {
                PrintAccessMethodResults(ServerResults);
            }

            if (CliResults != null && ServerResults != null)
            {
                Console.WriteLine("=== COMPARISON ===");
                Console.WriteLine($"CLI vs Server Latency Ratio: {CliResults.OverallAverageLatencyMs / ServerResults.OverallAverageLatencyMs:F2}x");
                Console.WriteLine($"CLI Success Rate: {(double)CliResults.SuccessfulOperations / CliResults.TotalOperations * 100:F1}%");
                Console.WriteLine($"Server Success Rate: {(double)ServerResults.SuccessfulOperations / ServerResults.TotalOperations * 100:F1}%");
            }
        }

        private void PrintAccessMethodResults(AccessMethodResults results)
        {
            Console.WriteLine($"--- {results.MethodName} ---");
            Console.WriteLine($"Total Operations: {results.TotalOperations}");
            Console.WriteLine($"Successful Operations: {results.SuccessfulOperations}");
            Console.WriteLine($"Failed Operations: {results.FailedOperations}");
            Console.WriteLine($"Overall Average Latency: {results.OverallAverageLatencyMs:F2}ms");
            Console.WriteLine($"Overall Min Latency: {results.OverallMinLatencyMs}ms");
            Console.WriteLine($"Overall Max Latency: {results.OverallMaxLatencyMs}ms");
            Console.WriteLine($"Overall Median Latency: {results.OverallMedianLatencyMs:F2}ms");
            Console.WriteLine();
        }
    }

    public class AccessMethodResults
    {
        public string MethodName { get; set; } = string.Empty;
        public Dictionary<string, QueryResults> QueryResults { get; set; } = new Dictionary<string, QueryResults>();
        public double OverallAverageLatencyMs { get; set; }
        public long OverallMinLatencyMs { get; set; }
        public long OverallMaxLatencyMs { get; set; }
        public double OverallMedianLatencyMs { get; set; }
        public int TotalOperations { get; set; }
        public int SuccessfulOperations { get; set; }
        public int FailedOperations { get; set; }
    }

    public class QueryResults
    {
        public double AverageLatencyMs { get; set; }
        public long MinLatencyMs { get; set; }
        public long MaxLatencyMs { get; set; }
        public double MedianLatencyMs { get; set; }
        public int SuccessfulOperations { get; set; }
    }
}