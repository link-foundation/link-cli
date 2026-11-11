using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>The established uint remote store using the shared selected-address protocol.</summary>
public sealed partial class RemoteLinks : RemoteLinks<uint>, ITriplets<uint>
{
    /// <summary>Uses an existing connection, which the store then owns.</summary>
    public RemoteLinks(LinksClient client) : base(client, new()) { }

    /// <summary>Connects to a server at an address written as host:port.</summary>
    public new static RemoteLinks Connect(string endPoint, ILinoProtocol protocol) => new(LinksClient.Connect(endPoint, protocol));

    /// <summary>Connects to a server at any IPC endpoint, such as <c>unix:/tmp/links.sock</c> or <c>shm:/tmp/links</c>.</summary>
    public new static RemoteLinks ConnectEndpoint(string endpoint, ILinoProtocol protocol) =>
        new(LinksClient.ConnectEndpoint(endpoint, protocol));

    /// <summary>Runs an established uint operation and returns its reply.</summary>
    public IReadOnlyList<LinoLink> Execute(LinksOperation operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        return ExecuteDocument(operation.ToDocument());
    }
    /// <summary>
    /// Asks the server to push the changes other connections make to the links matching <paramref name="restriction"/>
    /// and returns the number of the subscription, which <see cref="ReceiveNotification"/> reports with them.
    /// </summary>
    public uint Subscribe(LinksRestriction restriction)
    {
        ArgumentNullException.ThrowIfNull(restriction);
        return LinksOperation.ParseSubscription(Execute(new LinksOperation.Subscribe(restriction)));
    }

    /// <summary>Stops a subscription; changes already pushed for it are still received.</summary>
    public void Unsubscribe(uint subscription) => Execute(new LinksOperation.Unsubscribe(subscription));

    /// <summary>
    /// The next change pushed for a subscription of this store, waiting up to <paramref name="timeout"/> for one;
    /// null when none arrived in time.
    /// </summary>
    public LinksNotification? ReceiveNotification(TimeSpan timeout)
    {
        lock (_clientLock)
        {
            return _client.ReceiveNotification(timeout) is { } document ? LinksOperation.ParseNotification(document) : null;
        }
    }


}
