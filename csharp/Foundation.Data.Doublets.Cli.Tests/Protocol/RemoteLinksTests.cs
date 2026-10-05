using Foundation.Data.Doublets.Cli.Protocol;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary><see cref="RemoteLinks"/> is a drop-in replacement for a local store over every protocol.</summary>
public sealed class RemoteLinksTests
{
    /// <summary>
    /// The README walkthrough plus the cases that cascade: a merge into an existing doublet and the deletion of a
    /// link that others use.
    /// </summary>
    private static readonly string[] Queries =
    {
        "() ((1 1))",
        "() ((2 2))",
        "((1: 1 1)) ((1: 1 2))",
        "((1: 1 2)) ((1: 2 1))",
        "((($i: $s $t)) (($i: $t $s)))",
        "((2: 2 2)) ((2: 1 1))",
        "() ((father: father father))",
        "() ((mother: mother mother))",
        "() ((child: father mother))",
        "((child: father mother)) ((child: father mother))",
        "((child: father mother)) ((child: mother father))",
        "((father: father father)) ()",
        "((1 *)) ()",
    };

    private sealed class LocalStore : IDisposable
    {
        private readonly string _databaseFilename = Path.GetTempFileName();

        public LocalStore() => Links = new NamedTypesDecorator<uint>(_databaseFilename);

        public NamedTypesDecorator<uint> Links { get; }

        public void Dispose()
        {
            Links.Dispose();
            File.Delete(_databaseFilename);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(_databaseFilename));
        }
    }

    /// <summary>The net changes of every query, the way <c>clink --changes</c> reports them.</summary>
    private static List<List<(DoubletLink Before, DoubletLink After)>> RunQueries(INamedTypesLinks<uint> links) =>
        Queries.Select(query => NetChanges(links, record => AdvancedMixedQueryProcessor.ProcessQuery(links, new AdvancedMixedQueryProcessor.Options
        {
            Query = query,
            AutoCreateMissingReferences = true,
            ChangesHandler = record,
        }))).ToList();

    private static List<(DoubletLink Before, DoubletLink After)> NetChanges(INamedTypesLinks<uint> links, Action<WriteHandler<uint>> write)
    {
        var steps = new List<(DoubletLink Before, DoubletLink After)>();
        write((before, after) =>
        {
            steps.Add((new DoubletLink(before), new DoubletLink(after)));
            return links.Constants.Continue;
        });
        return ChangesSimplifier.SimplifyChanges(steps).ToList();
    }

    /// <summary>Every link with its name, the way <c>clink --after</c> prints the store.</summary>
    private static List<string> NamedLinks(INamedTypesLinks<uint> links)
    {
        string Part(uint link) => links.GetName(link) ?? link.ToString();
        return links.All(new DoubletLink(links.Constants.Any, links.Constants.Any, links.Constants.Any))
            .Select(link => new DoubletLink(link))
            .Select(link => $"({Part(link.Index)}: {Part(link.Source)} {Part(link.Target)})")
            .ToList();
    }

    [Fact]
    public void TheQueryProcessorGivesTheSameResultsOnARemoteStore()
    {
        using var local = new LocalStore();
        var expectedChanges = RunQueries(local.Links);
        var expectedLinks = NamedLinks(local.Links);
        // Deleting father cascades to child, and ((1 *)) () deletes what is left of the numbered links.
        Assert.Equal(new[] { "(mother: mother mother)" }, expectedLinks);

        foreach (var protocol in RunningServer.Protocols())
        {
            using var server = new RunningServer();
            using var remote = server.Remote(protocol);

            Assert.Equal(expectedChanges, RunQueries(remote));
            Assert.Equal(expectedLinks, NamedLinks(remote));
        }
    }

    /// <summary>
    /// Runs every <see cref="INamedTypesLinks{TLinkAddress}"/> call, through the extensions code uses it by, and
    /// returns what each call answered, so a remote store is held to exactly the answers of a local one.
    /// </summary>
    private static List<object?> Exercise(INamedTypesLinks<uint> links)
    {
        var answers = new List<object?>();
        void Write(Func<WriteHandler<uint>, uint> write) =>
            answers.AddRange(NetChanges(links, record => answers.Add(write(record))).Cast<object?>());

        answers.Add(links.Create());
        answers.Add(links.GetOrCreate(1u, 1u));
        answers.Add(links.GetOrCreate(1u, 1u));
        answers.Add(links.SearchOrDefault(1u, 1u));
        answers.Add(links.SearchOrDefault(2u, 2u));
        answers.Add(new DoubletLink(links.GetLink(2u)));
        answers.Add(links.Exists(1u));
        answers.Add(links.Exists(9u));
        answers.Add(links.Count());

        answers.Add(links.Create());
        answers.Add(links.Update(3u, 2u, 2u));
        Write(record => links.Update(new LinkAddress<uint>(3), new DoubletLink(3, 1, 1), record));
        answers.Add(links.Exists(3u));

        answers.Add(links.SetName(2, "pair"));
        answers.Add(links.GetName(2));
        answers.Add(links.GetByName("pair"));
        answers.Add(links.GetByName("none"));
        links.RemoveName(2);
        answers.Add(links.GetName(2));

        Write(record => links.Delete(new LinkAddress<uint>(1), record));
        answers.Add(links.Count());
        return answers;
    }

    [Fact]
    public void EveryNamedTypesLinksCallMatchesALocalStore()
    {
        using var local = new LocalStore();
        var expected = Exercise(local.Links);
        // Updating (3: 2 2) into the existing (2: 1 1) merges it away.
        Assert.Contains((new DoubletLink(3, 2, 2), default(DoubletLink)), expected);
        // Deleting 1 deletes (2: 1 1), which uses it, too.
        Assert.Equal(
            new object?[] { (new DoubletLink(2, 1, 1), default(DoubletLink)), (new DoubletLink(1, 0, 0), default(DoubletLink)) },
            expected.SkipLast(1).TakeLast(2));

        foreach (var protocol in RunningServer.Protocols())
        {
            using var server = new RunningServer();
            using var remote = server.Remote(protocol);

            Assert.Equal(expected, Exercise(remote));
        }
    }

    /// <summary>
    /// The requests and replies of <c>docs/protocol/links-operations.txt</c>, which the Rust tests replay against
    /// the Rust server too.
    /// </summary>
    private static IEnumerable<(string Request, string Reply)> Conversation()
    {
        var lines = File.ReadAllLines(Path.Combine(AppContext.BaseDirectory, "Protocol", "links-operations.txt"))
            .Where(line => line.Length > 0 && !line.StartsWith('#'))
            .ToList();
        for (var start = 0; start < lines.Count;)
        {
            var end = lines.FindIndex(start + 1, line => line.StartsWith("> ", StringComparison.Ordinal));
            end = end < 0 ? lines.Count : end;
            yield return (lines[start][2..], string.Join('\n', lines.Skip(start + 1).Take(end - start - 1)));
            start = end;
        }
    }

    [Fact]
    public void ServersAnswerTheSharedConversationOverEveryProtocol()
    {
        var conversation = Conversation().ToList();
        Assert.Equal(19, conversation.Count);
        foreach (var protocol in RunningServer.Protocols())
        {
            using var server = new RunningServer();
            using var client = server.Client(protocol);
            foreach (var (request, reply) in conversation)
            {
                if (reply.StartsWith("! ", StringComparison.Ordinal))
                {
                    var error = Assert.Throws<LinoProtocolException>(() => client.QueryText(request));
                    Assert.EndsWith(reply[2..], error.Message);
                }
                else
                {
                    Assert.Equal((request, reply), (request, client.QueryText(request)));
                }
            }
        }
    }

    [Fact]
    public void TheRawLinksInterfaceWorksOverEveryProtocol()
    {
        foreach (var protocol in RunningServer.Protocols())
        {
            using var server = new RunningServer();
            using var remote = server.Remote(protocol);
            var constants = remote.Constants;
            var any = constants.Any;

            var created = new List<(IList<uint>? Before, IList<uint>? After)>();
            var flow = remote.Create(null, (before, after) =>
            {
                created.Add((before, after));
                return constants.Continue;
            });
            Assert.Equal(constants.Continue, flow);
            Assert.Equal(new (IList<uint>?, IList<uint>?)[] { (null, new DoubletLink(1, 0, 0)) }, created);
            remote.Create(Array.Empty<uint>(), null);

            remote.Update(new uint[] { 1 }, new uint[] { 1, 1, 2 }, null);
            remote.Update(new uint[] { 2 }, new uint[] { 2, 2, 1 }, null);
            Assert.Equal(new DoubletLink(1, 1, 2), new DoubletLink(remote.GetLink(1u)));

            Assert.Equal(2u, remote.Count(null));
            Assert.Equal(2u, remote.Count(new[] { any }));
            Assert.Equal(1u, remote.Count(new uint[] { 2 }));
            Assert.Equal(0u, remote.Count(new uint[] { 3 }));
            Assert.Equal(2u, remote.Count(new[] { any, 1u }));
            Assert.Equal(1u, remote.Count(new[] { any, 1u, any }));
            Assert.Equal(1u, remote.Count(new[] { any, any, 1u }));
            Assert.Equal(0u, remote.Count(new[] { any, 2u, 2u }));

            var seen = new List<DoubletLink>();
            flow = remote.Each(new[] { any, any, any }, link =>
            {
                seen.Add(new DoubletLink(link));
                return constants.Break;
            });
            Assert.Equal(constants.Break, flow);
            Assert.Equal(new[] { new DoubletLink(1, 1, 2) }, seen);
            Assert.Equal(constants.Continue, remote.Each(null, null));

            var deleted = new List<(IList<uint>? Before, IList<uint>? After)>();
            flow = remote.Delete(new uint[] { 1 }, (before, after) =>
            {
                deleted.Add((before, after));
                return constants.Break;
            });
            Assert.Equal(constants.Break, flow);
            Assert.Single(deleted);
            Assert.Equal(0u, remote.Count(null));
        }
    }

    [Fact]
    public void MisshapenCallsAreRejectedBeforeAnythingIsSent()
    {
        using var server = new RunningServer();
        using var remote = server.Remote(new TextLinoProtocol());

        Assert.Throws<ArgumentException>(() => remote.Update(Array.Empty<uint>(), new uint[] { 1, 1, 1 }, null));
        Assert.Throws<ArgumentException>(() => remote.Update(new uint[] { 1 }, new uint[] { 1, 1 }, null));
        Assert.Throws<ArgumentException>(() => remote.Delete(null, null));
        Assert.Throws<ArgumentNullException>(() => remote.Execute(null!));
        Assert.Throws<ArgumentNullException>(() => new RemoteLinks(null!));
    }

    [Fact]
    public void ServerFailuresAreProtocolExceptions()
    {
        using var server = new RunningServer();
        using var remote = server.Remote(new TextLinoProtocol());

        var missing = Assert.Throws<LinoProtocolException>(() => remote.Update(7u, 1u, 1u));
        Assert.Equal(LinoProtocolErrorKind.Remote, missing.Kind);
        Assert.EndsWith("Link not found: 7", missing.Message);
        Assert.Throws<LinoProtocolException>(() => remote.Delete(7u));
        var tooLong = Assert.Throws<LinoProtocolException>(() => remote.Count(new uint[] { 1, 2, 3, 4 }));
        Assert.Equal(LinoProtocolErrorKind.Remote, tooLong.Kind);
        Assert.Equal(0u, remote.Count(null));
    }

    [Fact]
    public void ALostConnectionThrows()
    {
        using var server = new RunningServer();
        using var remote = server.Remote(new TextLinoProtocol());
        server.Stop();

        Assert.Throws<LinoProtocolException>(() => remote.Count(null));
    }

    [Fact]
    public void OperationsRoundTripThroughTheirDocuments()
    {
        LinksOperation[] operations =
        {
            new LinksOperation.Count(LinksRestriction.All),
            new LinksOperation.Count(LinksRestriction.Of(1)),
            new LinksOperation.Each(LinksRestriction.Of(null, 2)),
            new LinksOperation.Each(LinksRestriction.Of(1, null, 3)),
            new LinksOperation.Create(1, 2),
            new LinksOperation.Update(3, 4, 5),
            new LinksOperation.Delete(6),
            new LinksOperation.GetName(7),
            new LinksOperation.SetName(8, "a name"),
            new LinksOperation.GetByName("a name"),
            new LinksOperation.RemoveName(9),
        };
        foreach (var protocol in RunningServer.Protocols())
        {
            foreach (var operation in operations)
            {
                using var wire = new MemoryStream();
                protocol.WriteDocument(wire, operation.ToDocument());
                var document = protocol.ReadDocument(LinoStreamReader.FromBytes(wire.ToArray()))!;

                Assert.Equal(operation, LinksOperation.FromDocument(document));
            }
        }
    }

    [Theory]
    [InlineData("")]
    [InlineData("() ((1 1))")]
    [InlineData("((1: 1 1)) ()")]
    [InlineData("(unknown: 1)")]
    [InlineData("(count: 1 2)")]
    [InlineData("(count: 1)\n(count: 1)")]
    public void OnlyOperationShapedDocumentsAreOperations(string query) =>
        Assert.Null(LinksOperation.FromDocument(LinoFormat.ParseDocument(query)));

    [Theory]
    [InlineData("(count: (1 2 3 4))")]
    [InlineData("(each: (x))")]
    [InlineData("(create: (1))")]
    [InlineData("(update: (1 2))")]
    [InlineData("(delete: x)")]
    [InlineData("(delete: -1)")]
    [InlineData("(get-name: (1 2))")]
    [InlineData("(set-name: (1))")]
    [InlineData("(set-name: (1 (a b)))")]
    [InlineData("(get-by-name: (a b))")]
    [InlineData("(remove-name: 4294967296)")]
    public void MalformedOperationsAreRejected(string query)
    {
        var error = Assert.Throws<LinoProtocolException>(() => LinksOperation.FromDocument(LinoFormat.ParseDocument(query)));
        Assert.Equal(LinoProtocolErrorKind.Malformed, error.Kind);
    }

    [Theory]
    [InlineData("(count: 1 2)")]
    [InlineData("(count: x)")]
    [InlineData("(1 2)")]
    [InlineData("((1 2) x)")]
    public void MalformedRepliesAreRejected(string reply)
    {
        var document = LinoFormat.ParseDocument(reply);
        Assert.Throws<LinoProtocolException>(() => LinksOperation.ParseCount(document));
        Assert.Throws<LinoProtocolException>(() => LinksOperation.ParseChanges(document));
    }

    [Fact]
    public void RepliesParseBackToWhatTheyCarry()
    {
        Assert.Equal(3u, LinksOperation.ParseCount(LinoFormat.ParseDocument("(count: 3)")));
        Assert.Equal(
            new[] { new DoubletLink(1, 2, 3) },
            LinksOperation.ParseLinks(LinoFormat.ParseDocument("(1: 2 3)")));
        Assert.Equal(
            new[] { (default(DoubletLink), new DoubletLink(1, 0, 0)), (new DoubletLink(1, 0, 0), default) },
            LinksOperation.ParseChanges(LinoFormat.ParseDocument("() ((1: 0 0))\n((1: 0 0)) ()")));
        Assert.Equal("a name", LinksOperation.ParseName(LinoFormat.ParseDocument("(name: 'a name')")));
        Assert.Null(LinksOperation.ParseName(Array.Empty<Link.Foundation.Links.Notation.Link<string>>()));
        Assert.Equal(5u, LinksOperation.ParseLinkReply(LinoFormat.ParseDocument("(link: 5)")));
        Assert.Null(LinksOperation.ParseLinkReply(Array.Empty<Link.Foundation.Links.Notation.Link<string>>()));
    }

    [Theory]
    [InlineData(new uint[0], true)]
    [InlineData(new[] { Any }, true)]
    [InlineData(new uint[] { 1 }, true)]
    [InlineData(new uint[] { 2 }, false)]
    [InlineData(new[] { Any, 2u }, true)]
    [InlineData(new[] { Any, 3u }, true)]
    [InlineData(new[] { Any, 1u }, false)]
    [InlineData(new uint[] { 1, 2, 3 }, true)]
    [InlineData(new[] { Any, 3u, Any }, false)]
    [InlineData(new[] { Any, Any, Any, Any }, false)]
    public void RestrictionsMatchLikeTheRawLinksInterface(uint[] parts, bool matches)
    {
        var restriction = new LinksRestriction(parts.Select(part => part == Any ? null : (uint?)part).ToList());

        Assert.Equal(matches, restriction.Matches(new DoubletLink(1, 2, 3)));
    }

    private const uint Any = uint.MaxValue - 3;

    [Fact]
    public void RestrictionsAreValuesWrittenAsLinoGroups()
    {
        Assert.Equal(LinksRestriction.Of(1, null, 3), new LinksRestriction(new uint?[] { 1, null, 3 }.ToList()));
        Assert.Equal(LinksRestriction.Of(1, null, 3).GetHashCode(), LinksRestriction.Of(1, null, 3).GetHashCode());
        Assert.NotEqual(LinksRestriction.Of(1, null), LinksRestriction.Of(1, null, 3));
        Assert.False(LinksRestriction.All.Equals(null));
        Assert.Equal("(1 * 3)", LinksRestriction.Of(1, null, 3).ToString());
    }

    /// <summary>
    /// Restricting by source or target visits a link once even when both halves match, where
    /// <c>UnitedMemoryLinks</c> visits a self-referencing link once per usage.
    /// </summary>
    [Fact]
    public void ALinkUsingAnAddressTwiceMatchesOnce()
    {
        using var local = new LocalStore();
        var point = local.Links.GetOrCreate(1u, 1u);
        Assert.Equal(1u, point);
        var any = local.Links.Constants.Any;
        Assert.Equal(2u, local.Links.Count(new[] { any, point }));

        using var server = new RunningServer();
        using var remote = server.Remote(new TextLinoProtocol());
        remote.GetOrCreate(1u, 1u);

        Assert.Equal(1u, remote.Count(new[] { any, point }));
        Assert.Equal(new[] { new DoubletLink(1, 1, 1) }, remote.All(new[] { any, point }).Select(link => new DoubletLink(link)));
    }
}
