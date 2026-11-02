using System.Globalization;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Data.Doublets;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// A store served to many sessions, each with its subject (issue #149): what a
/// <see cref="LinksServer"/> gives every connection, without the network.
/// </summary>
/// <remarks>
/// <para>
/// Without <see cref="LinksServerOptions.Permissions"/> every session sees and
/// changes the whole store, exactly as before permissions existed. With them,
/// a session sees the store through a <see cref="PermissionsDecorator{TLinkAddress}"/>
/// of its subject, and every request is atomic: one that fails is undone
/// whole, the writes the store made on its own (cascades, merges) and the
/// names it took included, even where they touched links the subject cannot see.
/// </para>
/// <para>
/// <c>(auth: token)</c> sets the subject of the session to the one
/// <see cref="LinksServerOptions.Authenticate"/> gives the token, and answers
/// <c>(subject: i)</c>; an unknown token, or a server without an authenticator,
/// answers <c>(error: 'Forbidden: authentication failed')</c>.
/// </para>
/// </remarks>
public sealed class LinksSessions
{
    private readonly object _lock = new();
    private readonly List<LinksSession> _sessions = new();

    /// <param name="links">The store.</param>
    /// <param name="options">The options of the server; only the permissions, authentication and references matter.</param>
    public LinksSessions(INamedTypesLinks<uint> links, LinksServerOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(links);
        Options = options ?? new LinksServerOptions();
        Links = links;
        if (Options.Permissions is not null)
        {
            Undoable = new UndoableLinksDecorator<uint>(links);
        }
    }

    /// <summary>The store.</summary>
    public INamedTypesLinks<uint> Links { get; }

    /// <summary>The options.</summary>
    public LinksServerOptions Options { get; }

    /// <summary>The store as the sessions write it when there are permissions: every write kept until the request ends.</summary>
    internal UndoableLinksDecorator<uint>? Undoable { get; }

    internal object Lock => _lock;

    /// <summary>The sessions that are open.</summary>
    public IReadOnlyList<LinksSession> Sessions
    {
        get
        {
            lock (_lock)
            {
                return _sessions.ToList();
            }
        }
    }

    /// <summary>Opens a session of <paramref name="subject"/>, by default <see cref="LinksServerOptions.Subject"/>.</summary>
    public LinksSession Open(uint? subject = null)
    {
        lock (_lock)
        {
            var session = new LinksSession(this, subject ?? Options.Subject);
            _sessions.Add(session);
            return session;
        }
    }

    internal void Close(LinksSession session)
    {
        lock (_lock)
        {
            _sessions.Remove(session);
        }
    }

    /// <summary>The links at <paramref name="deleted"/> are gone: no session but <paramref name="origin"/> owns them any more.</summary>
    internal void Forget(LinksSession origin, IReadOnlyCollection<uint> deleted)
    {
        foreach (var session in _sessions)
        {
            if (session != origin)
            {
                foreach (var index in deleted)
                {
                    session.View?.Forget(index);
                }
            }
        }
    }
}

/// <summary>A connection to a <see cref="LinksSessions"/> store: its requests run as its subject.</summary>
public sealed class LinksSession : IDisposable
{
    private readonly LinksSessions _host;

    internal LinksSession(LinksSessions host, uint subject)
    {
        _host = host;
        Subject = subject;
        if (host.Options.Permissions is { } policy)
        {
            View = new PermissionsDecorator<uint>(host.Undoable!, policy, () => Subject);
        }
    }

    /// <summary>The subject the requests run as; the null link (0) is a guest.</summary>
    public uint Subject { get; private set; }

    /// <summary>The store as the subject sees it, when there are permissions.</summary>
    public PermissionsDecorator<uint>? View { get; }

    /// <summary>Answers one request document.</summary>
    public IReadOnlyList<LinoLink> Execute(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        lock (_host.Lock)
        {
            if (document is [{ Id: "auth", Values: [var token] }])
            {
                return Authenticate(token);
            }
            if (View is null)
            {
                return LinksServer.ExecuteRequest(_host.Links, document, _host.Options.AutoCreateMissingReferences);
            }
            var store = _host.Undoable!;
            store.Begin();
            IReadOnlyList<LinoLink> reply;
            try
            {
                reply = Request(View, store, document);
            }
            catch
            {
                Undo(store);
                throw;
            }
            if (LinksServer.ErrorMessage(reply) is not null)
            {
                Undo(store);
                return reply;
            }
            var deleted = store.Log.Changes
                .Where(change => change.Before is not null && change.After is null)
                .Select(change => change.Before!.Value.Index)
                .ToHashSet();
            store.Commit();
            _host.Forget(this, deleted);
            return reply;
        }
    }

    /// <summary>Answers a request written as LiNo text, as the text protocol does.</summary>
    public string Request(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        IReadOnlyList<LinoLink> document;
        try
        {
            document = LinoFormat.ParseDocument(text);
        }
        catch (Exception error) when (error is not OutOfMemoryException)
        {
            return LinoFormat.FormatDocument(LinksServer.ErrorDocument(error.Message));
        }
        return LinoFormat.FormatDocument(Execute(document));
    }

    /// <summary>Closes the session.</summary>
    public void Dispose() => _host.Close(this);

    private void Undo(UndoableLinksDecorator<uint> store)
    {
        foreach (var created in store.Rollback())
        {
            View!.Forget(created);
        }
    }

    private IReadOnlyList<LinoLink> Authenticate(LinoLink token)
    {
        var found = token is { Values: null, Id: { } text } && _host.Options.Authenticate is { } authenticate
            ? authenticate(text)
            : null;
        if (found is not { } subject)
        {
            return LinksServer.ErrorDocument("Forbidden: authentication failed");
        }
        Subject = subject;
        return new[]
        {
            LinoFormat.Link("subject", new List<LinoLink> { LinoFormat.Reference(subject.ToString(CultureInfo.InvariantCulture)) }),
        };
    }

    /// <summary>
    /// A request as the subject sees the store: names of links it cannot read are
    /// not found, and references are checked in the store below.
    /// </summary>
    private IReadOnlyList<LinoLink> Request(PermissionsDecorator<uint> view, INamedTypesLinks<uint> store, IReadOnlyList<LinoLink> document)
    {
        LinksOperation? operation;
        try
        {
            operation = LinksOperation.FromDocument(document);
        }
        catch (Exception error) when (error is not OutOfMemoryException)
        {
            return LinksServer.ErrorDocument(error.Message);
        }
        switch (operation)
        {
            case LinksOperation.GetName get when !view.Exists(get.Link):
                return Array.Empty<LinoLink>();
            case LinksOperation.GetByName find:
                var link = view.GetByName(find.Name);
                return link != 0 && view.Exists(link) ? find.Execute(view) : Array.Empty<LinoLink>();
            default:
                return LinksServer.ExecuteRequest(view, document, _host.Options.AutoCreateMissingReferences, store);
        }
    }
}
