using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>The established uint remote store using the shared selected-address protocol.</summary>
public sealed class RemoteLinks : RemoteLinks<uint>
{
    /// <summary>Uses an existing connection, which the store then owns.</summary>
    public RemoteLinks(LinksClient client) : base(client, new()) { }

    /// <summary>Connects to a server at an address written as host:port.</summary>
    public new static RemoteLinks Connect(string endPoint, ILinoProtocol protocol) => new(LinksClient.Connect(endPoint, protocol));

    /// <summary>Runs an established uint operation and returns its reply.</summary>
    public IReadOnlyList<LinoLink> Execute(LinksOperation operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        return ExecuteDocument(operation.ToDocument());
    }
}
