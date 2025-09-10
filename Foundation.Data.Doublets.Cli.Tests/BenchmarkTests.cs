using Xunit;
using System.IO;
using System.Threading.Tasks;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class BenchmarkTests
    {
        [Fact]
        public async Task BenchmarkRunner_ShouldRunSuccessfully()
        {
            // Arrange
            var tempDbPath = Path.GetTempFileName();
            try
            {
                var benchmarkRunner = new BenchmarkRunner(tempDbPath, trace: false);
                var options = new BenchmarkOptions
                {
                    TestQueries = new List<string> { "() ((1 1))" }, // Simple create operation
                    IterationsPerQuery = 2,
                    WarmupIterations = 1,
                    ServerPort = 8081 // Use different port to avoid conflicts
                };

                // Act
                var results = await benchmarkRunner.RunBenchmarkAsync(options);

                // Assert
                Assert.NotNull(results);
                Assert.NotNull(results.CliResults);
                Assert.NotNull(results.ServerResults);
                Assert.True(results.CliResults.TotalOperations > 0);
                Assert.True(results.ServerResults.TotalOperations > 0);
                Assert.True(results.CliResults.OverallAverageLatencyMs >= 0);
                Assert.True(results.ServerResults.OverallAverageLatencyMs >= 0);
            }
            finally
            {
                if (File.Exists(tempDbPath))
                {
                    File.Delete(tempDbPath);
                }
            }
        }

        [Fact]
        public void BenchmarkOptions_ShouldHaveDefaults()
        {
            // Arrange & Act
            var options = new BenchmarkOptions();

            // Assert
            Assert.NotNull(options.TestQueries);
            Assert.Empty(options.TestQueries);
            Assert.Equal(10, options.IterationsPerQuery);
            Assert.Equal(3, options.WarmupIterations);
            Assert.Equal(8080, options.ServerPort);
        }

        [Fact]
        public void BenchmarkResults_ShouldPrintReport()
        {
            // Arrange
            var results = new BenchmarkResults
            {
                CliResults = new AccessMethodResults
                {
                    MethodName = "CLI Test",
                    TotalOperations = 10,
                    SuccessfulOperations = 10,
                    FailedOperations = 0,
                    OverallAverageLatencyMs = 5.0
                },
                ServerResults = new AccessMethodResults
                {
                    MethodName = "Server Test",
                    TotalOperations = 10,
                    SuccessfulOperations = 8,
                    FailedOperations = 2,
                    OverallAverageLatencyMs = 10.0
                }
            };

            // Act & Assert - Should not throw
            results.PrintReport();
        }

        [Fact]
        public void LinoProtocolClient_ShouldConnectAndDisconnect()
        {
            // Arrange
            var client = new LinoProtocolClient("localhost", 8082);

            // Act & Assert - Should not throw when disposing without connecting
            client.Dispose();

            // Test multiple dispose calls
            client.Dispose();
        }

        [Fact]
        public void LinoRequest_ShouldHaveProperties()
        {
            // Arrange & Act
            var request = new LinoRequest
            {
                Query = "() ((1 1))",
                RequestId = 123,
                Timestamp = DateTime.UtcNow
            };

            // Assert
            Assert.Equal("() ((1 1))", request.Query);
            Assert.Equal(123, request.RequestId);
            Assert.True(request.Timestamp != default);
        }

        [Fact]
        public void LinoResponse_ShouldHaveProperties()
        {
            // Arrange & Act
            var response = new LinoResponse
            {
                Success = true,
                ProcessingTimeMs = 42,
                ChangesCount = 1,
                Changes = new List<ChangeInfo>
                {
                    new ChangeInfo { Before = null, After = "(1: 1 1)" }
                }
            };

            // Assert
            Assert.True(response.Success);
            Assert.Equal(42, response.ProcessingTimeMs);
            Assert.Equal(1, response.ChangesCount);
            Assert.Single(response.Changes);
            Assert.Null(response.Changes[0].Before);
            Assert.Equal("(1: 1 1)", response.Changes[0].After);
        }
    }
}