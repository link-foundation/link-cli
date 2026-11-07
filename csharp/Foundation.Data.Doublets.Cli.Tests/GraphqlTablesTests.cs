using System.CommandLine;
using System.Net;
using System.Net.Sockets;
using System.Text;
using Foundation.Data.Doublets.Cli.Graphql;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>The relations and comparisons of the Deep schema, and the command that starts its server.</summary>
public sealed class GraphqlTablesTests
{
    public static TheoryData<string, string> Relations()
    {
        var all = new TheoryData<string, string>();
        foreach (var name in new[] { "from", "to", "type", "string", "number", "object", "in", "out", "typed", "can_action", "can_object", "can_rule", "can_subject", "down", "up", "tree", "root", "selected", "selectors", "_by_group", "_by_item", "_by_path_item", "_by_root" })
        {
            all.Add("links", name);
        }
        foreach (var table in new[] { "strings", "numbers", "objects" })
        {
            all.Add(table, "link");
        }
        foreach (var name in new[] { "action", "object", "rule", "subject" })
        {
            all.Add("can", name);
        }
        foreach (var name in new[] { "link", "parent", "root", "tree", "by_tree", "by_link", "by_parent", "by_position", "by_root" })
        {
            all.Add("tree", name);
        }
        foreach (var name in new[] { "item", "selector", "query" })
        {
            all.Add("selectors", name);
        }
        foreach (var name in new[] { "dist", "execution_provider", "handler", "isolation_provider", "src" })
        {
            all.Add("handlers", name);
        }
        return all;
    }

    [Theory]
    [MemberData(nameof(Relations))]
    public void EveryRelationJoinsColumnsThatExist(string table, string name)
    {
        var relation = Tables.RelationOf(table, name);
        Assert.NotNull(relation);
        Assert.Contains(relation.Value.Local, Tables.Columns(table));
        Assert.Contains(relation.Value.Target, Tables.Names);
        Assert.Contains(relation.Value.Remote, Tables.Columns(relation.Value.Target));
    }

    [Fact]
    public void EveryTableHasColumnsAndOthersHaveNone()
    {
        Assert.All(Tables.Names, table => Assert.NotEmpty(Tables.Columns(table)));
        Assert.Empty(Tables.Columns("other"));
        Assert.Null(Tables.RelationOf("links", "other"));
        Assert.Null(Tables.RelationOf("other", "link"));
    }

    [Theory]
    [InlineData("a", "a", 0)]
    [InlineData("a", "b", -1)]
    [InlineData("b", "a", 1)]
    [InlineData("a", "ab", -1)]
    [InlineData("ab", "a", 1)]
    // UTF-16 puts this emoji before U+FFFD; its code point comes after, as in UTF-8.
    [InlineData("😀", "�", 1)]
    [InlineData("�", "😀", -1)]
    [InlineData("😀", "😁", -1)]
    public void TextComparesInCodePointOrder(string left, string right, int sign) =>
        Assert.Equal(sign, Math.Sign(Tables.CompareText(left, right)));

    [Fact]
    public void OnlyValuesOfOneKindCompare()
    {
        Assert.Equal(-1, Tables.Compare(Json.Str("a"), Json.Str("b")));
        Assert.Equal(1, Tables.Compare(Json.Bool(true), Json.Bool(false)));
        Assert.Equal(0, Tables.Compare(Json.Int(1), Json.Float(1.0)));
        Assert.Null(Tables.Compare(Json.Str("1"), Json.Int(1)));
        Assert.Null(Tables.Compare(Json.Int(1), Json.Str("1")));
        Assert.Null(Tables.Compare(Json.Bool(true), Json.Int(1)));
        Assert.Null(Tables.Compare(Json.Float(double.NaN), Json.Int(1)));
        Assert.Null(Tables.Compare(Json.Int(1), Json.Float(double.NaN)));
    }

    [Theory]
    [InlineData("abc", "abc", false, true)]
    [InlineData("abc", "a%", false, true)]
    [InlineData("abc", "%c", false, true)]
    [InlineData("abc", "a_c", false, true)]
    [InlineData("abc", "a%b%", false, true)]
    [InlineData("abc", "%%%", false, true)]
    [InlineData("abcbd", "%b_", false, true)]
    [InlineData("abc", "ab", false, false)]
    [InlineData("ab", "abc", false, false)]
    [InlineData("abc", "%d", false, false)]
    [InlineData("ABC", "abc", false, false)]
    [InlineData("ABC", "abc", true, true)]
    [InlineData("😀b", "_b", false, true)]
    public void LikeMatchesRunsAndSingleCharacters(string text, string pattern, bool insensitive, bool matches) =>
        Assert.Equal(matches, Tables.Like(text, pattern, insensitive));

    [Theory]
    [InlineData("_eq", "1", "1", true)]
    [InlineData("_eq", "1", "1.0", true)]
    [InlineData("_eq", "1", "2", false)]
    [InlineData("_eq", "[1]", "[1]", true)]
    [InlineData("_neq", "1", "2", true)]
    [InlineData("_neq", "1", "1", false)]
    [InlineData("_neq", "[1]", "[1]", false)]
    [InlineData("_neq", "1", "null", false)]
    [InlineData("_gt", "2", "1", true)]
    [InlineData("_gt", "1", "\"a\"", false)]
    [InlineData("_gte", "1", "1", true)]
    [InlineData("_lt", "1", "2", true)]
    [InlineData("_lte", "1", "1", true)]
    [InlineData("_in", "1", "[1,2]", true)]
    [InlineData("_in", "3", "[1,2]", false)]
    [InlineData("_in", "1", "1", true)]
    [InlineData("_nin", "3", "[1,2]", true)]
    [InlineData("_nin", "1", "[1,2]", false)]
    [InlineData("_like", "\"abc\"", "\"a%\"", true)]
    [InlineData("_like", "1", "\"%\"", true)]
    [InlineData("_like", "\"abc\"", "1", false)]
    [InlineData("_nlike", "\"abc\"", "\"a%\"", false)]
    [InlineData("_ilike", "\"ABC\"", "\"a%\"", true)]
    [InlineData("_nilike", "\"ABC\"", "\"a%\"", false)]
    [InlineData("_regex", "\"abc\"", "\"^a\"", true)]
    [InlineData("_regex", "\"ABC\"", "\"^a\"", false)]
    [InlineData("_regex", "\"abc\"", "\"(\"", false)]
    [InlineData("_regex", "\"aa\"", "\"(a)\\\\1\"", false)]
    [InlineData("_nregex", "\"abc\"", "\"^a\"", false)]
    [InlineData("_iregex", "\"ABC\"", "\"^a\"", true)]
    [InlineData("_niregex", "\"ABC\"", "\"^a\"", false)]
    [InlineData("_contains", "{\"a\":1,\"b\":2}", "{\"a\":1}", true)]
    [InlineData("_contains", "{\"a\":1}", "{\"b\":1}", false)]
    [InlineData("_contains", "{\"a\":1}", "{\"a\":2}", false)]
    [InlineData("_contains", "{\"a\":{\"b\":1,\"c\":2}}", "{\"a\":{\"b\":1}}", true)]
    [InlineData("_contains", "[1,2]", "[2]", true)]
    [InlineData("_contains", "[1,2]", "[3]", false)]
    [InlineData("_contains", "[1,2]", "1", false)]
    [InlineData("_contains", "1", "1.0", true)]
    [InlineData("_contains", "null", "null", false)]
    [InlineData("_contained_in", "{\"a\":1}", "{\"a\":1,\"b\":2}", true)]
    [InlineData("_has_key", "{\"a\":1}", "\"a\"", true)]
    [InlineData("_has_key", "{\"a\":1}", "\"b\"", false)]
    [InlineData("_is_null", "null", "true", true)]
    [InlineData("_is_null", "1", "true", false)]
    [InlineData("_is_null", "1", "false", true)]
    [InlineData("_is_null", "null", "false", false)]
    [InlineData("_is_null", "null", "1", false)]
    public void OperatorsCompareAColumnWithTheirArgument(string op, string value, string argument, bool passes)
    {
        // A number has no text: patterns see it as empty.
        Assert.Equal(passes, Tables.Passes(Json.Parse(value), op, Json.Parse(argument)));
    }

    [Theory]
    [InlineData("_eq")]
    [InlineData("_neq")]
    [InlineData("_in")]
    [InlineData("_nin")]
    [InlineData("_like")]
    [InlineData("_contains")]
    public void SqlNullComparesFalseToEverything(string op) =>
        Assert.False(Tables.Passes(Json.Null, op, Json.Null));

    [Fact]
    public void AnUnknownOperatorIsRefusedOnlyForAValue()
    {
        Assert.Contains("_other", Assert.Throws<FormatException>(() => Tables.Passes(Json.Int(1), "_other", Json.Int(1))).Message, StringComparison.Ordinal);
        Assert.False(Tables.Passes(Json.Null, "_other", Json.Int(1)));
    }

    private static string Database() => Path.Combine(Path.GetTempPath(), $"clink-graphql-command-{Guid.NewGuid():N}.links");

    private static void Remove(string database)
    {
        foreach (var file in Directory.GetFiles(Path.GetTempPath(), Path.GetFileName(database) + "*"))
        {
            File.Delete(file);
        }
    }

    private static (GraphqlCommand Command, ParseResult Parsed) Parse(params string[] arguments)
    {
        var command = new GraphqlCommand();
        var root = new RootCommand();
        command.AddTo(root);
        return (command, root.Parse(arguments));
    }

    [Fact]
    public void TheCommandIsOffUnlessAsked()
    {
        var (command, parsed) = Parse();
        Assert.False(command.Enabled(parsed));
        (command, parsed) = Parse("--graphql");
        Assert.True(command.Enabled(parsed));
    }

    [Theory]
    [InlineData("127.0.0.1:0", "--jwt-secret", "s3", "--graphql-admin-secret", "root")]
    [InlineData("localhost:0")]
    public void TheCommandLoadsPackagesAndServes(string address, params string[] options)
    {
        var package = Path.Combine(Path.GetTempPath(), $"clink-graphql-package-{Guid.NewGuid():N}.json");
        File.WriteAllText(package, """[{"id":1,"type_id":1,"from_id":0,"to_id":0}]""");
        var database = Database();
        try
        {
            var (command, parsed) = Parse(["--graphql", "--deep-package", package, .. options]);
            using var links = new NamedTypesDecorator<uint>(database, false);
            string? answer = null;
            Thread? client = null;
            var exit = command.Run(parsed, address, links, null, server =>
            {
                // Serve starts after this callback, so the request runs beside it.
                client = new Thread(() =>
                {
                    answer = Ask(server.LocalEndpoint);
                    server.Stop();
                })
                { IsBackground = true };
                client.Start();
            });
            client!.Join();
            Assert.Equal(0, exit);
            Assert.Equal("""{"data":{"links":[{"id":1}]}}""", answer);
        }
        finally
        {
            File.Delete(package);
            Remove(database);
        }
    }

    private static string Ask(IPEndPoint endpoint)
    {
        using var client = new TcpClient();
        client.Connect(endpoint);
        client.ReceiveTimeout = 20000;
        var stream = client.GetStream();
        const string Body = """{"query":"query { links(where: {id: {_eq: 1}}) { id } }"}""";
        stream.Write(Encoding.ASCII.GetBytes($"POST /gql HTTP/1.1\r\nContent-Length: {Body.Length}\r\n\r\n{Body}"));
        using var reader = new StreamReader(stream, Encoding.UTF8);
        var length = 0;
        while (reader.ReadLine() is { Length: > 0 } line)
        {
            if (line.StartsWith("Content-Length:", StringComparison.OrdinalIgnoreCase))
            {
                length = int.Parse(line["Content-Length:".Length..].Trim(), System.Globalization.CultureInfo.InvariantCulture);
            }
        }
        var buffer = new char[length];
        reader.ReadBlock(buffer);
        return new string(buffer);
    }

    [Fact]
    public void APackageThatIsNotJsonNamesItsFile()
    {
        var package = Path.Combine(Path.GetTempPath(), $"clink-graphql-package-{Guid.NewGuid():N}.json");
        File.WriteAllText(package, "{not json");
        var database = Database();
        try
        {
            var (command, parsed) = Parse("--graphql", "--deep-package", package);
            using var links = new NamedTypesDecorator<uint>(database, false);
            var error = Assert.Throws<InvalidOperationException>(() => command.Run(parsed, "127.0.0.1:0", links, null));
            Assert.Contains(package, error.Message, StringComparison.Ordinal);
        }
        finally
        {
            File.Delete(package);
            Remove(database);
        }
    }
}
