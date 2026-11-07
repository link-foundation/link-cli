using System.CommandLine;
using System.Net;
using System.Text;
using Foundation.Data.Doublets.Cli.Protocol;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>The options of <c>--serve --graphql</c> and the server they start.</summary>
public sealed class GraphqlCommand
{
    private readonly Option<bool> _enabled = new("--graphql")
    {
        Description = "With --serve: serve Hasura-compatible GraphQL for Deep over HTTP and WebSocket instead of the links protocol",
        DefaultValueFactory = _ => false,
    };

    private readonly Option<string?> _jwtSecret = new("--jwt-secret")
    {
        Description = "With --graphql: the HS256 secret of tokens; without it a random one lasts as long as the process",
    };

    private readonly Option<string?> _adminSecret = new("--graphql-admin-secret")
    {
        Description = "With --graphql: the value of x-hasura-admin-secret that bypasses permissions",
    };

    private readonly Option<string[]> _packages = new("--deep-package")
    {
        Description = "With --graphql: a JSON file of rows {id, type_id, from_id, to_id, string?, number?, object?} to load when starting; may be repeated",
    };

    public void AddTo(RootCommand command)
    {
        command.Options.Add(_enabled);
        command.Options.Add(_jwtSecret);
        command.Options.Add(_adminSecret);
        command.Options.Add(_packages);
    }

    public bool Enabled(ParseResult parsed) => parsed.GetValue(_enabled);

    /// <summary>Loads the packages, binds and serves until Ctrl+C, or until <paramref name="started"/>'s server is stopped.</summary>
    public int Run(ParseResult parsed, string address, INamedTypesLinks<uint> links, PermissionPolicy<uint>? policy, Action<GraphqlServer>? started = null)
    {
        var store = new DeepStore(links, policy);
        foreach (var file in parsed.GetValue(_packages) ?? [])
        {
            try
            {
                store.LoadPackage(Json.Parse(File.ReadAllText(file)));
            }
            catch (FormatException error)
            {
                throw new InvalidOperationException($"{file}: {error.Message}", error);
            }
        }
        var secret = parsed.GetValue(_jwtSecret);
        var settings = new Settings(
            secret is null ? Settings.RandomSecret() : Encoding.UTF8.GetBytes(secret),
            parsed.GetValue(_adminSecret));
        var (host, port) = LinksServer.ParseEndPoint(address);
        var bound = IPAddress.TryParse(host, out var parsedHost) ? parsedHost : Dns.GetHostAddresses(host).First();
        using var server = new GraphqlServer(new IPEndPoint(bound, port));
        void StopOnInterrupt(object? _, ConsoleCancelEventArgs eventArgs)
        {
            eventArgs.Cancel = true;
            server.Stop();
        }
        Console.CancelKeyPress += StopOnInterrupt;
        try
        {
            Console.WriteLine($"clink GraphQL server listening on {server.LocalEndpoint}");
            Console.Out.Flush();
            started?.Invoke(server);
            server.Serve(store, settings);
        }
        finally
        {
            Console.CancelKeyPress -= StopOnInterrupt;
        }
        return 0;
    }
}
