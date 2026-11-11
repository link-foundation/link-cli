using Foundation.Data.Doublets.Cli.Ipc;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// The endpoints of <c>--serve</c>, <c>--forward</c> and <c>--graphql</c> (issue #140), as the Rust <c>clink</c>
/// handles them: <c>--serve</c> repeats to serve one store on several endpoints at once, and <c>--forward</c> relays
/// every connection of the <c>--serve</c> endpoints to another endpoint instead of opening a database.
/// </summary>
internal static class CliServe
{
    /// <summary>
    /// <c>--serve A --forward B</c>: relays every connection accepted on the <c>--serve</c> endpoints, byte for byte,
    /// to a new connection to <paramref name="target"/> until a stop signal; no database is opened, so relays stack
    /// any transports in either protocol.
    /// </summary>
    internal static int RunRelay(IReadOnlyList<string> serve, string target, bool hasQuery, bool graphql, bool trace)
    {
        if (serve.Count == 0)
        {
            Console.Error.WriteLine("--forward relays the connections of --serve; give --serve as well.");
            return 1;
        }
        if (hasQuery)
        {
            Console.Error.WriteLine("--forward does not take a query; send queries with --connect.");
            return 1;
        }
        if (graphql)
        {
            Console.Error.WriteLine("--forward and --graphql cannot be used together.");
            return 1;
        }
        Endpoint endpoint;
        IIpcListener listener;
        try
        {
            endpoint = Endpoint.Parse(target);
            listener = IpcTransports.ListenAll(serve);
        }
        catch (Exception error) when (error is IOException or ArgumentException)
        {
            Console.Error.WriteLine($"Error: {error.Message}");
            return 1;
        }
        using (listener)
        {
            using var stopSignals = new StopSignals(listener.Closer());
            PrintBanner(listener.Endpoints);
            if (trace)
            {
                Console.Error.WriteLine($"clink: forwarding to {endpoint}");
            }
            IpcTransports.Relay(listener, endpoint);
        }
        return 0;
    }

    /// <summary>
    /// Prints <c>clink server listening on ENDPOINT</c> once per endpoint: on standard output, or on standard error
    /// when one endpoint is <c>stdio</c>, whose standard output carries the protocol.
    /// </summary>
    internal static void PrintBanner(IReadOnlyList<string> endpoints)
    {
        var stdio = endpoints.Any(IsStdio);
        var writer = stdio ? Console.Error : Console.Out;
        foreach (var endpoint in endpoints)
        {
            writer.WriteLine($"clink server listening on {endpoint}");
        }
        writer.Flush();
    }

    /// <summary>
    /// The TCP <c>host:port</c> that <c>--graphql</c> serves HTTP on, or null after reporting on standard error why
    /// the <c>--serve</c> endpoints give none: GraphQL takes exactly one, and over TCP.
    /// </summary>
    internal static string? GraphqlAddress(IReadOnlyList<string> serve)
    {
        if (serve.Count != 1)
        {
            Console.Error.WriteLine("--graphql serves HTTP on one TCP address; give --serve once.");
            return null;
        }
        Endpoint endpoint;
        try
        {
            endpoint = Endpoint.Parse(serve[0]);
        }
        catch (IOException error)
        {
            Console.Error.WriteLine($"Error: {error.Message}");
            return null;
        }
        if (endpoint.Kind != "tcp")
        {
            Console.Error.WriteLine($"--graphql serves HTTP over TCP only, not over `{endpoint.Kind}:`.");
            return null;
        }
        return endpoint.Address;
    }

    private static bool IsStdio(string endpoint)
    {
        try
        {
            return Endpoint.Parse(endpoint).Kind == "stdio";
        }
        catch (IOException)
        {
            return false;
        }
    }
}
