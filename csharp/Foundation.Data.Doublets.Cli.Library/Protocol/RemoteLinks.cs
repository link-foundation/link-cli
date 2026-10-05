using Platform.Data;
using Platform.Delegates;

using DoubletLink = Platform.Data.Doublets.Link<uint>;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// A links store served by a <see cref="LinksServer"/>, usable wherever a local store is.
/// </summary>
/// <remarks>
/// <para>
/// It implements the same interface as a local store, <see cref="INamedTypesLinks{TLinkAddress}"/>, so code written
/// against it — the <see cref="AdvancedMixedQueryProcessor"/> and every <c>ILinks</c> extension included — switches
/// from a local file to a server by swapping one value:
/// </para>
/// <code>
/// static void Run(INamedTypesLinks&lt;uint&gt; links) =&gt;
///     AdvancedMixedQueryProcessor.ProcessQuery(links, new() { Query = "() ((1 1))" });
///
/// using (var local = new NamedTypesDecorator&lt;uint&gt;("db.links")) Run(local);
/// using (var remote = RemoteLinks.Connect("127.0.0.1:8080", new TextLinoProtocol())) Run(remote);
/// </code>
/// <para>
/// Every call is one <see cref="LinksOperation"/> round trip, in the documents the Rust <c>RemoteLinks</c> sends,
/// so either port's client works with either port's server. A write handler sees the net change of every link a
/// write touched, not the steps the store behind the server took. A failure of the connection or of the server is a
/// <see cref="LinoProtocolException"/>. The calls are serialised, so one instance may be shared between threads.
/// </para>
/// </remarks>
public sealed class RemoteLinks : INamedTypesLinks<uint>, IDisposable
{
    private readonly LinksClient _client;
    private readonly object _clientLock = new();

    /// <summary>Uses an existing connection, which the store then owns.</summary>
    public RemoteLinks(LinksClient client)
    {
        ArgumentNullException.ThrowIfNull(client);
        _client = client;
    }

    /// <summary>Connects to a server at an address written as <c>host:port</c>.</summary>
    public static RemoteLinks Connect(string endPoint, ILinoProtocol protocol) => new(LinksClient.Connect(endPoint, protocol));

    /// <summary>The constants of the <c>UnitedMemoryLinks</c> a server stores its links in.</summary>
    public LinksConstants<uint> Constants { get; } = new();

    /// <summary>Runs one operation on the server and returns its reply.</summary>
    public IReadOnlyList<LinoLink> Execute(LinksOperation operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        lock (_clientLock)
        {
            return _client.Request(operation.ToDocument());
        }
    }

    /// <inheritdoc/>
    public uint Count(IList<uint>? restriction) =>
        LinksOperation.ParseCount(Execute(new LinksOperation.Count(Restriction(restriction))));

    /// <inheritdoc/>
    public uint Each(IList<uint>? restriction, ReadHandler<uint>? handler)
    {
        foreach (var link in LinksOperation.ParseLinks(Execute(new LinksOperation.Each(Restriction(restriction)))))
        {
            if (handler?.Invoke(link) == Constants.Break)
            {
                return Constants.Break;
            }
        }
        return Constants.Continue;
    }

    /// <summary>Creates an empty <c>(index: 0 0)</c> link, like every <c>UnitedMemoryLinks</c>.</summary>
    public uint Create(IList<uint>? substitution, WriteHandler<uint>? handler) =>
        Replay(new LinksOperation.Create(0, 0), handler);

    /// <summary>Updates the link at the index of <paramref name="restriction"/> to <c>(index source target)</c>.</summary>
    /// <exception cref="ArgumentException">The restriction or substitution is not of that shape.</exception>
    public uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler) =>
        (restriction, substitution) is ([var index, ..], [_, var source, var target])
            ? Replay(new LinksOperation.Update(index, source, target), handler)
            : throw new ArgumentException("an update needs an index and an (index source target) substitution", nameof(substitution));

    /// <summary>Deletes the link at the index of <paramref name="restriction"/>, and every link that refers to it.</summary>
    /// <exception cref="ArgumentException">The restriction holds no index.</exception>
    public uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler) =>
        restriction is [var index, ..]
            ? Replay(new LinksOperation.Delete(index), handler)
            : throw new ArgumentException("a delete needs an index", nameof(restriction));

    /// <inheritdoc/>
    public string? GetName(uint link) => LinksOperation.ParseName(Execute(new LinksOperation.GetName(link)));

    /// <inheritdoc/>
    public uint SetName(uint link, string name) =>
        LinksOperation.ParseLinkReply(Execute(new LinksOperation.SetName(link, name)))
            ?? throw new LinoProtocolException(LinoProtocolErrorKind.Malformed, "the set-name reply holds no link");

    /// <summary>The link named <paramref name="name"/>, or <c>0</c> when there is none.</summary>
    public uint GetByName(string name) => LinksOperation.ParseLinkReply(Execute(new LinksOperation.GetByName(name))) ?? 0;

    /// <inheritdoc/>
    public void RemoveName(uint link) => Execute(new LinksOperation.RemoveName(link));

    /// <summary>Closes the connection.</summary>
    public void Dispose() => _client.Dispose();

    private LinksRestriction Restriction(IList<uint>? restriction) =>
        new((restriction ?? Array.Empty<uint>()).Select(part => part == Constants.Any ? null : (uint?)part).ToList());

    /// <summary>
    /// Runs a <c>create</c>, <c>update</c> or <c>delete</c> and feeds its changes to <paramref name="handler"/> until
    /// it asks to stop, passing a missing side as null the way <c>UnitedMemoryLinks</c> does.
    /// </summary>
    private uint Replay(LinksOperation operation, WriteHandler<uint>? handler)
    {
        foreach (var (before, after) in LinksOperation.ParseChanges(Execute(operation)))
        {
            if (handler?.Invoke(Side(before), Side(after)) == Constants.Break)
            {
                return Constants.Break;
            }
        }
        return Constants.Continue;
    }

    private static IList<uint>? Side(DoubletLink link) => link.IsNull() ? null : (IList<uint>)link;
}
