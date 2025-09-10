namespace Foundation.Data.Doublets.Cli.Benchmarks;

public class Program
{
    public static async Task Main(string[] args)
    {
        await SimpleBenchmark.RunBenchmarksAsync();
    }
}