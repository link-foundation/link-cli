using System.Globalization;
using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using World = Foundation.Data.Doublets.Cli.Tests.PermissionsDecoratorTests.World;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>
/// Sessions of a links server, each with its subject (issue #149): the
/// permission cases of <c>server.test.js</c> of <c>@link-foundation/ui-links</c>;
/// the Rust twin is <c>rust/tests/links_sessions_tests.rs</c>, case for case.
/// </summary>
public sealed class LinksSessionsTests
{
    private static LinksSessions Serve(World w) => new(w.Links, new LinksServerOptions { Permissions = w.Policy });

    private static string S(uint value) => value.ToString(CultureInfo.InvariantCulture);

    private static uint Next(World w) => w.Links.Count(Array.Empty<uint>()) + 1;

    [Fact]
    public void AuthenticatesWithTheCallback()
    {
        using var file = new TemporaryStore();
        var sessions = new LinksSessions(file.Links, new LinksServerOptions
        {
            Authenticate = token => token == "secret" ? 7u : null,
        });
        using var writer = sessions.Open();
        Assert.Equal("(error: 'Forbidden: authentication failed')", writer.Request("(auth: wrong)"));
        Assert.Equal(0u, writer.Subject);
        Assert.Equal("(subject: 7)", writer.Request("(auth: secret)"));
        Assert.Equal(7u, writer.Subject);
        Assert.Equal("() ((1: 1 1))", writer.Request("() ((1 1))"));
        Assert.Single(sessions.Sessions);
        writer.Dispose();
        Assert.Empty(sessions.Sessions);
    }

    [Fact]
    public void RefusesAuthenticationWithoutAnAuthenticator()
    {
        using var file = new TemporaryStore();
        using var session = new LinksSessions(file.Links).Open();
        Assert.Equal("(error: 'Forbidden: authentication failed')", session.Request("(auth: secret)"));
    }

    [Fact]
    public void ServesEachSubjectWhatItsPermissionsAllow()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var sessions = Serve(w);
            using var alice = sessions.Open(w.V["alice"]);
            using var boss = sessions.Open(w.V["boss"]);
            var secret = S(w.V["secret"]);
            var note = S(w.T["Note"]);
            var seen = w.As(w.V["alice"]).Count(Array.Empty<uint>());
            Assert.Equal($"(count: {S(seen)})", alice.Request("(count: ())"));
            Assert.Equal($"(error: 'Link not found: {secret}')", alice.Request($"(update: ({secret} {note} {secret}))"));
            // set-name answers where the names store keeps the name.
            Assert.StartsWith("(link: ", alice.Request($"(set-name: ({S(w.V["note"])} 'mine'))"), StringComparison.Ordinal);
            Assert.Equal("mine", w.Links.GetName(w.V["note"]));
            Assert.Equal("", alice.Request("(get-by-name: 'secret')"));
            Assert.Equal("", alice.Request($"(get-name: {secret})"));
            Assert.Equal("", alice.Request("((secret: * *)) ()"));
            Assert.Equal("(name: secret)", boss.Request($"(get-name: {secret})"));
            Assert.Equal($"(link: {secret})", boss.Request("(get-by-name: 'secret')"));
            Assert.Equal(
                $"(({secret}: {S(w.T["Secret"])} {secret})) (({secret}: {note} {secret}))",
                boss.Request($"(update: ({secret} {note} {secret}))"));
            Assert.Equal($"(count: {S(seen + 1)})", alice.Request("(count: ())"));
            using var guest = sessions.Open();
            Assert.Equal("(error: 'Forbidden: create')", guest.Request("(create: (0 0))"));
        });
    }

    [Fact]
    public void LetsASubjectTypeTheLinksItCreatesAsTypesItMayNotRead()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            using var alice = Serve(w).Open(w.V["alice"]);
            var note = S(w.T["Note"]);
            // alice may write notes, but may not read the link of the type Note.
            Assert.Equal("", alice.Request($"(each: ({note}))"));
            var n = Next(w);
            Assert.Equal($"() (({S(n)}: 0 0))", alice.Request("(create: (0 0))"));
            Assert.Equal($"(({S(n)}: 0 0)) (({S(n)}: {note} {S(n)}))", alice.Request($"(update: ({S(n)} {note} {S(n)}))"));
            // (n: Note n) is there already: a store of unique doublets would merge a
            // second one into it, so the next note points to another link.
            var other = S(w.V["alice"]);
            Assert.Equal($"() (({S(n + 1)}: {note} {other}))", alice.Request($"(create: ({note} {other}))"));
            // The names of types alice cannot read resolve all the same.
            Assert.Equal($"() (({S(n + 2)}: Note {S(n + 1)}))", alice.Request($"() ((Note {S(n + 1)}))"));
        });
    }

    [Fact]
    public void UndoesAFailedRequestAndReportsItsOwnError()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            using var alice = Serve(w).Open(w.V["alice"]);
            var note = S(w.T["Note"]);
            var n = S(Next(w));
            alice.Request("(create: (0 0))");
            // The update is allowed before, then refused as it would be after:
            // undoing it needs no permission, and the error is the one of the request.
            // (A store of doublets takes a missing target, so ui-links' 9999 does not fail here.)
            Assert.Equal($"(error: 'Forbidden: update {n}')", alice.Request($"(update: ({n} {S(w.T["Secret"])} {n}))"));
            Assert.Equal($"({n}: 0 0)", alice.Request($"(each: ({n}))"));
            Assert.Equal($"(({n}: 0 0)) (({n}: {note} {n}))", alice.Request($"(update: ({n} {note} {n}))"));
        });
    }

    [Fact]
    public void UndoesEveryPartOfAFailedSubstitutionIncludingHiddenOnes()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var sessions = Serve(w);
            using var alice = sessions.Open(w.V["alice"]);
            var note = S(w.T["Note"]);
            var before = w.All(w.Links);
            var names = before.ToDictionary(link => link.Index, link => w.Links.GetName(link.Index));
            // The first change is allowed, the second is not: neither stays.
            var reply = alice.Request($"() (({note} {note}) ({S(w.T["Secret"])} {S(w.T["Secret"])}))");
            Assert.StartsWith("(error: 'Forbidden: ", reply, StringComparison.Ordinal);
            Assert.Equal(before, w.All(w.Links));
            Assert.Equal(names, before.ToDictionary(link => link.Index, link => w.Links.GetName(link.Index)));
            // Deleting a note deletes the links that use it and its name, writes
            // of the store alice does not ask for; the request fails at its
            // forbidden update after the delete, and all of it is undone.
            var k = S(Next(w));
            var noteLink = S(w.V["note"]);
            var shared = S(w.V["shared"]);
            var sharedType = S(w.T["Shared"]);
            Assert.Equal($"() (({k}: {sharedType} {noteLink}))", alice.Request($"(create: ({sharedType} {noteLink}))"));
            before = w.All(w.Links);
            reply = alice.Request($"(({noteLink}: * *) ({shared}: * *)) (({shared}: {S(w.T["Secret"])} {shared}))");
            Assert.Equal($"(error: 'Forbidden: update {shared}')", reply);
            Assert.Equal(before, w.All(w.Links));
            Assert.Equal("note", w.Links.GetName(w.V["note"]));
            Assert.Equal("shared", w.Links.GetName(w.V["shared"]));
            // Allowed, the same request deletes both and the name.
            using var boss = sessions.Open(w.V["boss"]);
            boss.Request($"(({noteLink}: * *) ({shared}: * *)) (({shared}: {S(w.T["Secret"])} {shared}))");
            Assert.False(w.Links.Exists(w.V["note"]));
            Assert.False(w.Links.Exists(uint.Parse(k, CultureInfo.InvariantCulture)));
            Assert.Null(w.Links.GetName(w.V["note"]));
        });
    }

    [Fact]
    public void DoesNotGiveASubjectALinkMadeWhereItsOwnWasDeleted()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var sessions = Serve(w);
            using var alice = sessions.Open(w.V["alice"]);
            using var boss = sessions.Open(w.V["boss"]);
            var n = S(Next(w));
            alice.Request("(create: (0 0))");
            Assert.Equal($"({n}: 0 0)", alice.Request($"(each: ({n}))"));
            boss.Request($"(delete: {n})");
            Assert.Equal($"() (({n}: 0 0))", boss.Request("(create: (0 0))"));
            Assert.Equal("", alice.Request($"(each: ({n}))"));
            Assert.Equal($"(error: 'Link not found: {n}')", alice.Request($"(update: ({n} 0 0))"));
        });
    }

    [Fact]
    public void ServesRulesInAStoreOfTheirOwnAlike()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var sessions = Serve(w);
            using var alice = sessions.Open(w.V["alice"]);
            var expected = w.As(w.V["alice"]).Count(Array.Empty<uint>());
            Assert.Equal($"(count: {S(expected)})", alice.Request("(count: ())"));
            Assert.Equal("", alice.Request("((secret: * *)) ()"));
            // set-name answers where the names store keeps the name.
            Assert.StartsWith("(link: ", alice.Request($"(set-name: ({S(w.V["note"])} 'mine'))"), StringComparison.Ordinal);
            Assert.Equal("mine", w.Links.GetName(w.V["note"]));
        }, separateRules: true);
    }

    [Fact]
    public void WithoutPermissionsEverySessionSeesTheWholeStore()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            using var alice = new LinksSessions(w.Links).Open(w.V["alice"]);
            Assert.Equal($"(count: {S(w.Links.Count(Array.Empty<uint>()))})", alice.Request("(count: ())"));
            Assert.Equal($"(link: {S(w.V["secret"])})", alice.Request("(get-by-name: 'secret')"));
        });
    }

    [Fact]
    public void AuthenticatesWithATokenTableInLinks()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var table = new LinksTokenTable(w.Links);
            Assert.Null(table.Authenticate("alice-token"));
            var link = table.Grant("alice-token", w.V["alice"]);
            table.Grant("alice-second-token", w.V["alice"]);
            table.Grant("boss-token", w.V["boss"]);
            Assert.Equal(w.V["alice"], table.Authenticate("alice-token"));
            Assert.Equal(w.V["alice"], table.Authenticate("alice-second-token"));
            Assert.Equal(w.V["boss"], table.Authenticate("boss-token"));
            Assert.Null(table.Authenticate("wrong"));
            // The store holds a hash of the token, never the token itself.
            Assert.Equal(LinksTokenTable.NameOf("alice-token"), w.Links.GetName(link));
            Assert.DoesNotContain("alice-token", w.Links.GetName(link), StringComparison.Ordinal);
            Assert.Equal(link, table.Grant("alice-token", w.V["boss"]));
            Assert.Equal(w.V["boss"], table.Authenticate("alice-token"));

            var sessions = new LinksSessions(w.Links, new LinksServerOptions { Permissions = w.Policy, Authenticate = table.Authenticate });
            using var session = sessions.Open();
            Assert.Equal("(error: 'Forbidden: create')", session.Request("(create: (0 0))"));
            Assert.Equal($"(subject: {S(w.V["alice"])})", session.Request("(auth: alice-second-token)"));
            Assert.Equal($"(count: {S(w.As(w.V["alice"]).Count(Array.Empty<uint>()))})", session.Request("(count: ())"));
            Assert.True(table.Revoke("alice-second-token"));
            Assert.False(table.Revoke("alice-second-token"));
            Assert.Null(table.Authenticate("alice-second-token"));
            Assert.Equal("(error: 'Forbidden: authentication failed')", session.Request("(auth: alice-second-token)"));
        });
    }

    [Fact]
    public void AuthenticatesOverTcp()
    {
        using var server = new RunningServer(new LinksServerOptions
        {
            Authenticate = token => token == "secret" ? 7u : null,
        });
        foreach (var protocol in RunningServer.Protocols())
        {
            using var client = server.Client(protocol);
            Assert.Equal(7u, client.Authenticate("secret"));
            var error = Assert.ThrowsAny<Exception>(() => client.Authenticate("wrong"));
            Assert.Contains("Forbidden: authentication failed", error.Message, StringComparison.Ordinal);
        }
    }

    private sealed class TemporaryStore : IDisposable
    {
        private readonly string _file = Path.GetTempFileName();

        public TemporaryStore()
        {
            Links = new NamedTypesDecorator<uint>(_file);
        }

        public NamedTypesDecorator<uint> Links { get; }

        public void Dispose()
        {
            Links.Dispose();
            File.Delete(_file);
            File.Delete(Path.ChangeExtension(_file, ".names.links"));
        }
    }
}
