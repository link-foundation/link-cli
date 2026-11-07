using Foundation.Data.Doublets.Cli.Protocol;
using Foundation.Data.Doublets.Cli.Engine;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

public sealed class CheckedSessionsTests
{
    private sealed class Checked(INamedTypesLinks<uint> links) : LinksDecoratorBase<uint>(links), INamedTypesLinks<uint>
    {
        public QueryLimits QueryLimits { get; } = new();
        public string? GetName(uint id) => links.GetName(id);
        public uint SetName(uint id, string name) => links.SetName(id, name);
        public uint GetByName(string name) => links.GetByName(name);
        public void RemoveName(uint id) => links.RemoveName(id);
    }

    [Fact]
    public void PermissionsAndUndoRetainCheckedAdmission()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var links = engine.Begin();
        var policy = new PermissionPolicy<uint>(links, links);
        var checkedLinks = new Checked(links);
        var undo = new UndoableLinksDecorator<uint>(checkedLinks);
        var view = new PermissionsDecorator<uint>(undo, policy, 0);
        Assert.Same(checkedLinks.QueryLimits, view.QueryLimits);
        using var session = new LinksSessions(checkedLinks, new LinksServerOptions { Permissions = policy }).Open();
        var document = LinksServer.QueryDocument(new string('x', checkedLinks.QueryLimits.MaxInputBytes + 1));
        Assert.Contains("byte limit", LinksServer.ErrorMessage(session.Execute(document))!, StringComparison.OrdinalIgnoreCase);
        var text = new string('(', 80) + "x" + new string(')', 80);
        Assert.Contains("error:", session.Request(text));
        LinoLink[] deep = new[] { LinoFormat.Reference("x") };
        for (var i = 0; i < 80; i++) deep = new[] { LinoFormat.Link(null, deep) };
        Assert.Contains("depth limit", LinksServer.ErrorMessage(session.Execute(deep))!, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void LosslessEnvelopeStillReportsChangesToSubscribers()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var links = engine.Begin();
        using var session = new LinksSessions(links).Open();
        var changes = new List<(Link<uint> Before, Link<uint> After)>();
        var reply = session.Execute(LinksServer.QueryDocument("() ((1: 1 1))"), changes);
        Assert.Null(LinksServer.ErrorMessage(reply));
        Assert.Equal((default(Link<uint>), new Link<uint>(1, 1, 1)), Assert.Single(changes));
    }
}
