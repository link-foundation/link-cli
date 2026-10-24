// Races TcpClient.GetStream() on one thread against TcpClient.Dispose() on
// another, as LinksServer did when Shutdown disposed a client whose worker was
// still getting its stream. Counts how each race ends.
//
// Usage: dotnet run -c Release -- [iterations]
//
// GetStream() checks Disposed, then Connected, then reads Client, and Client
// returns null once the TcpClient is disposed. A Dispose between the last two
// steps makes it call new NetworkStream(null), which throws
// ArgumentNullException: the exception that crashed the macOS test host.
using System.Net;
using System.Net.Sockets;

var iterations = args.Length > 0 ? int.Parse(args[0]) : 20_000;
using var listener = new TcpListener(IPAddress.Loopback, 0);
listener.Start();
var port = ((IPEndPoint)listener.LocalEndpoint).Port;
var outcomes = new Dictionary<string, int>();

for (var i = 0; i < iterations; i++)
{
    using var peer = new TcpClient();
    peer.Connect(IPAddress.Loopback, port);
    var accepted = listener.AcceptTcpClient();
    using var ready = new Barrier(2);
    var disposer = new Thread(() =>
    {
        ready.SignalAndWait();
        accepted.Dispose();
    });
    disposer.Start();
    ready.SignalAndWait();
    string outcome;
    try
    {
        accepted.GetStream().Dispose();
        outcome = "stream obtained";
    }
    catch (Exception error)
    {
        outcome = error.GetType().Name;
    }
    disposer.Join();
    outcomes[outcome] = outcomes.GetValueOrDefault(outcome) + 1;
}

Console.WriteLine($"{iterations} races on .NET {Environment.Version} ({System.Runtime.InteropServices.RuntimeInformation.OSDescription})");
foreach (var (outcome, count) in outcomes.OrderBy(pair => pair.Key))
{
    Console.WriteLine($"  {outcome}: {count}");
}
