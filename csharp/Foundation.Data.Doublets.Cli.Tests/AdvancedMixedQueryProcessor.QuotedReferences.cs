using Platform.Data;
using Platform.Data.Doublets;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests;

public partial class AdvancedMixedQueryProcessor
{
    [Theory]
    [InlineData("0")]
    [InlineData("1")]
    [InlineData("2")]
    [InlineData("99")]
    [InlineData("01")]
    [InlineData("+1")]
    [InlineData("4294967295")]
    public void QuotedNumbersAreNames(string name)
    {
        RunTestWithLinks(links =>
        {
            ProcessQueryStrict(links, "() ((a: a a) (b: b b))");
            foreach (var quote in new[] { "\"", "'", "`" })
            {
                ProcessQuery(links, $"() ((p: a {quote}{name}{quote}))");
                var named = links.GetByName(name);
                Assert.NotEqual(0u, named);
                Assert.NotEqual(links.GetByName("a"), named);
                Assert.NotEqual(links.GetByName("b"), named);
                Assert.Equal(named, new DoubletLink(links.GetLink(links.GetByName("p"))).Target);
            }
            ProcessQueryStrict(links, "() ((address: a 1))");
            Assert.Equal(1u, new DoubletLink(links.GetLink(links.GetByName("address"))).Target);
        });
    }

    [Fact]
    public void QuotedDefinitionsSupportStrictMatchingUpdatingAndDeletion()
    {
        RunTestWithLinks(links =>
        {
            ProcessQueryStrict(links, "() ((a: a a) (b: b b) (\"1\": \"1\" \"1\") ('0': '0' '0'))");
            var one = links.GetByName("1");
            ProcessQueryStrict(links, "() ((p: a \"1\"))");
            ProcessQueryStrict(links, "((p: a '1')) ((p: b '0'))");
            var p = links.GetByName("p");
            Assert.Equal(links.GetByName("b"), new DoubletLink(links.GetLink(p)).Source);
            Assert.Equal(links.GetByName("0"), new DoubletLink(links.GetLink(p)).Target);
            ProcessQueryStrict(links, "((\"1\": '1' `1`)) ((\"1\": a '0'))");
            Assert.Equal(one, links.GetByName("1"));
            Assert.Equal(links.GetByName("0"), new DoubletLink(links.GetLink(one)).Target);
            ProcessQueryStrict(links, "((p: b \"0\")) ()");
            Assert.False(links.Exists(p));
            Assert.True(links.Exists(1u));
        });
    }

    [Fact]
    public void MissingQuotedNumberRequiresItsNameEvenWhenAddressExists()
    {
        RunTestWithLinks(links =>
        {
            ProcessQueryStrict(links, "() ((a: a a))");
            var error = Assert.Throws<InvalidOperationException>(() =>
                ProcessQueryStrict(links, "() ((p: a \"1\"))"));
            Assert.Contains("'1'", error.Message);
            Assert.Contains("--auto-create-missing-references", error.Message);
            Assert.Equal(0u, links.GetByName("1"));
        });
    }

    [Fact]
    public void NumericNameRemainsStableWhenItsAddressIsAllocatedLater()
    {
        RunTestWithLinks(links =>
        {
            ProcessQuery(links, "() ((p: x '99'))");
            var named = links.GetByName("99");
            ProcessQuery(links, "() ((99: 99 99))");
            Assert.NotEqual(99u, named);
            ProcessQueryStrict(links, "() ((p: x \"99\"))");
            Assert.Equal(named, links.GetByName("99"));
            Assert.Equal(named, new DoubletLink(links.GetLink(links.GetByName("p"))).Target);
            ProcessQueryStrict(links, "() ((address: x 99))");
            Assert.Equal(99u, new DoubletLink(links.GetLink(links.GetByName("address"))).Target);
        });
    }

    [Theory]
    [InlineData("*")]
    [InlineData("$value")]
    [InlineData("a:")]
    [InlineData(" 1 ")]
    [InlineData(@"x\y")]
    [InlineData("雪")]
    [InlineData("~1{31}")]
    [InlineData("\0one\0")]
    [InlineData("\"one\"")]
    [InlineData("both'\"quotes")]
    public void QuotedAndEncodedReferencesPreserveLiteralNames(string name)
    {
        RunTestWithLinks(links =>
        {
            var literal = Link.Foundation.Links.Notation.ReferenceLiteral.Encode(name);
            ProcessQuery(links, $"() ((p: x {literal}))");
            var named = links.GetByName(name);
            Assert.NotEqual(0u, named);
            Assert.Equal(named, new DoubletLink(links.GetLink(links.GetByName("p"))).Target);
            // The same text quoted and encoded resolves to the same name.
            if (!name.Contains('\0'))
            {
                var quote = name.Contains('\'') ? "`" : "'";
                ProcessQueryStrict(links, $"() ((p: x {quote}{name}{quote}))");
                Assert.Equal(named, links.GetByName(name));
            }
        });
    }

    [Theory]
    [InlineData("''1''", "1")]
    [InlineData("``1``", "1")]
    [InlineData("\"\"\"1\"\"\"", "1")]
    [InlineData("'a''b'", "a'b")]
    [InlineData("''a''''b''", "a''b")]
    [InlineData("'line\nnext'", "line\nnext")]
    [InlineData("'# literal'", "# literal")]
    public void QueryQuotesFollowNotationDelimiterRules(string reference, string name)
    {
        RunTestWithLinks(links =>
        {
            ProcessQuery(links, $"() ((p: x {reference})) # comment\n");
            var named = links.GetByName(name);
            Assert.NotEqual(0u, named);
            Assert.Equal(named, new DoubletLink(links.GetLink(links.GetByName("p"))).Target);
        });
    }

    [Fact]
    public void ExportedNumericNamesCanBeReplayedAsAQuery()
    {
        RunTestWithLinks(source =>
        {
            ProcessQueryStrict(source, "() ((1: 1 1) ('1': '1' '1') ('0': '0' '0') ('42': '42' '42') (p: 1 '1'))");
            var exported = LinoDatabaseOutput.FormatDatabase(source);
            RunTestWithLinks(destination =>
            {
                ProcessQueryStrict(destination, $"() ({string.Join(" ", exported)})");
                Assert.Equal(exported, LinoDatabaseOutput.FormatDatabase(destination));
            });
        });
    }

    [Fact]
    public void NormalizedTriggerQueryKeepsQuotedNamesAndBareAddresses()
    {
        RunTestWithLinks(links =>
        {
            var parsed = PersistentTransformationDecorator.PersistentTransformationQuery.Parse(
                "() ((a: a a) (p: a '0') (q: a '1') (r: a 1))");
            ProcessQuery(links, parsed.Query);
            Assert.NotEqual(0u, links.GetByName("0"));
            Assert.NotEqual(0u, links.GetByName("1"));
            Assert.Equal(links.GetByName("1"), new DoubletLink(links.GetLink(links.GetByName("q"))).Target);
            Assert.Equal(1u, new DoubletLink(links.GetLink(links.GetByName("r"))).Target);
        });
    }
}
