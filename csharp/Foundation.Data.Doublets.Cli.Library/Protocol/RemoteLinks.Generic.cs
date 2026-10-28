using System.Numerics;
using Platform.Data;
using Platform.Delegates;

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
public class RemoteLinks<T> : INamedTypesLinks<T>, IDisposable
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly LinksClient _client;
    private readonly object _clientLock = new();

    /// <summary>Uses an existing connection, which the store then owns.</summary>
    public RemoteLinks(LinksClient client) : this(client, new((T.One, T.MaxValue))) { }

    private protected RemoteLinks(LinksClient client, LinksConstants<T> constants)
    {
        Constants = constants;
        ArgumentNullException.ThrowIfNull(client);
        _client = client;
    }

    /// <summary>Connects to a server at an address written as <c>host:port</c>.</summary>
    public static RemoteLinks<T> Connect(string endPoint, ILinoProtocol protocol) => new(LinksClient.Connect(endPoint, protocol));

    /// <summary>The constants of the <c>UnitedMemoryLinks</c> a server stores its links in.</summary>
    public LinksConstants<T> Constants { get; }

    /// <summary>Runs one operation on the server and returns its reply.</summary>
    public IReadOnlyList<LinoLink> Execute(LinksOperation<T> operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        return ExecuteDocument(operation.ToDocument());
    }

    private protected IReadOnlyList<LinoLink> ExecuteDocument(IReadOnlyList<LinoLink> document)
    {
        lock (_clientLock) return _client.Request(document);
    }

    /// <inheritdoc/>
    public T Count(IList<T>? restriction) =>
        LinksOperation<T>.ParseCount(Execute(new LinksOperation<T>.Count(Restriction(restriction))));

    /// <inheritdoc/>
    public T Each(IList<T>? restriction, ReadHandler<T>? handler)
    {
        foreach (var link in LinksOperation<T>.ParseLinks(Execute(new LinksOperation<T>.Each(Restriction(restriction)))))
        {
            if (handler?.Invoke(new LinkParts<T>(link.Index, link.Source, link.Target)) == Constants.Break)
            {
                return Constants.Break;
            }
        }
        return Constants.Continue;
    }

    /// <summary>Creates an empty <c>(index: 0 0)</c> link, like every <c>UnitedMemoryLinks</c>.</summary>
    public T Create(IList<T>? substitution, WriteHandler<T>? handler) =>
        Replay(new LinksOperation<T>.Create(T.Zero, T.Zero), handler);

    /// <summary>Updates the link at the index of <paramref name="restriction"/> to <c>(index source target)</c>.</summary>
    /// <exception cref="ArgumentException">The restriction or substitution is not of that shape.</exception>
    public T Update(IList<T>? restriction, IList<T>? substitution, WriteHandler<T>? handler) =>
        (restriction, substitution) is ([var index, ..], [_, var source, var target])
            ? Replay(new LinksOperation<T>.Update(index, source, target), handler)
            : throw new ArgumentException("an update needs an index and an (index source target) substitution", nameof(substitution));

    /// <summary>Deletes the link at the index of <paramref name="restriction"/>, and every link that refers to it.</summary>
    /// <exception cref="ArgumentException">The restriction holds no index.</exception>
    public T Delete(IList<T>? restriction, WriteHandler<T>? handler) =>
        restriction is [var index, ..]
            ? Replay(new LinksOperation<T>.Delete(index), handler)
            : throw new ArgumentException("a delete needs an index", nameof(restriction));

    /// <inheritdoc/>
    public string? GetName(T link) => LinksOperation<T>.ParseName(Execute(new LinksOperation<T>.GetName(link)));

    /// <inheritdoc/>
    public T SetName(T link, string name) =>
        LinksOperation<T>.ParseLinkReply(Execute(new LinksOperation<T>.SetName(link, name)))
            ?? throw new LinoProtocolException(LinoProtocolErrorKind.Malformed, "the set-name reply holds no link");

    /// <summary>The link named <paramref name="name"/>, or <c>0</c> when there is none.</summary>
    public T GetByName(string name) => LinksOperation<T>.ParseLinkReply(Execute(new LinksOperation<T>.GetByName(name))) ?? T.Zero;

    /// <inheritdoc/>
    public void RemoveName(T link) => Execute(new LinksOperation<T>.RemoveName(link));

    /// <summary>Closes the connection.</summary>
    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    /// <summary>Closes the owned connection when disposing managed resources.</summary>
    protected virtual void Dispose(bool disposing)
    {
        if (disposing) _client.Dispose();
    }

    private LinksRestriction<T> Restriction(IList<T>? restriction) =>
        new((restriction ?? Array.Empty<T>()).Select(part => part == Constants.Any ? null : (T?)part).ToList());

    /// <summary>
    /// Runs a <c>create</c>, <c>update</c> or <c>delete</c> and feeds its changes to <paramref name="handler"/> until
    /// it asks to stop, passing a missing side as null the way <c>UnitedMemoryLinks</c> does.
    /// </summary>
    private T Replay(LinksOperation<T> operation, WriteHandler<T>? handler)
    {
        foreach (var (before, after) in LinksOperation<T>.ParseChanges(Execute(operation)))
        {
            if (handler?.Invoke(Side(before), Side(after)) == Constants.Break)
            {
                return Constants.Break;
            }
        }
        return Constants.Continue;
    }

    private static IList<T>? Side(Platform.Data.Doublets.Link<T> link) =>
        link.Index == T.Zero && link.Source == T.Zero && link.Target == T.Zero
            ? null : new LinkParts<T>(link.Index, link.Source, link.Target);
}
