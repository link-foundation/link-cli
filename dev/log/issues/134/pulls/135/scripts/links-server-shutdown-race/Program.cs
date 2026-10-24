// Shuts a LinksServer down right after it accepted a connection, over and
// over, as LinksServerTests.ShuttingDownRightAfterAConnectionArrivesIsClean
// does 200 times. Before the fix the worker could call TcpClient.GetStream()
// while Shutdown disposed that client, which throws ArgumentNullException on
// the worker thread and kills the process (exit code 134 on Linux and macOS).
//
// Usage: dotnet run -c Release [-p:LinkCliRoot=<checkout>] -- [iterations]
using System.Net.Sockets;
using Foundation.Data.Doublets.Cli;
using Foundation.Data.Doublets.Cli.Protocol;

var iterations = args.Length > 0 ? int.Parse(args[0]) : 5_000;
var database = Path.GetTempFileName();
try
{
    using var links = new NamedTypesDecorator<uint>(database);
    for (var i = 1; i <= iterations; i++)
    {
        using var server = LinksServer.Bind("127.0.0.1", 0);
        var serving = new Thread(() => server.Serve(links)) { IsBackground = true };
        serving.Start();
        using (var client = new TcpClient("127.0.0.1", server.LocalEndPoint.Port))
        {
            server.Shutdown();
        }
        serving.Join();
        if (i % 1_000 == 0)
        {
            Console.WriteLine($"{i} shutdowns survived");
        }
    }
    Console.WriteLine($"all {iterations} shutdowns survived");
}
finally
{
    File.Delete(database);
    File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(database));
}
