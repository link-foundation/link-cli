using Foundation.Data.Doublets.Cli;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class LinoDatabaseInputTests
{
    [Theory]
    [InlineData("(c: c")]
    [InlineData("(c: c)")]
    [InlineData("(c: c c) trailing")]
    [InlineData("('c: c c)")]
    [InlineData("(c: '' c)")]
    [InlineData("(c: c \"\")")]
    [InlineData("(c: ~1{zz} c)")]
    [InlineData("(c: c ~1{c328})")]
    public void ImportText_InvalidLastLineLeavesLinksAndNamesUnchanged(string invalidLine)
    {
        foreach (var populated in new[] { false, true })
        {
            WithNamedLinks(links =>
            {
                if (populated)
                {
                    LinoDatabaseInput.ImportText(links, "(a: a a)\n(b: b b)\n(existing: a b)");
                }
                var before = LinoDatabaseOutput.FormatDatabase(links).ToArray();

                var error = Assert.Throws<FormatException>(() => LinoDatabaseInput.ImportText(
                    links, $"(a: b a)\n(new: fresh fresh)\n{invalidLine}"));

                Assert.Contains("line 3", error.Message);
                Assert.Equal(before, LinoDatabaseOutput.FormatDatabase(links));
                Assert.Equal(links.Constants.Null, links.GetByName("new"));
                Assert.Equal(links.Constants.Null, links.GetByName("fresh"));
            });
        }
    }

    [Theory]
    [InlineData("\n")]
    [InlineData("\r\n")]
    public void ImportText_InvalidInputAfterMultilineNamesLeavesLinksAndNamesUnchanged(string newline)
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, "(a: a a)\n(b: b b)\n(existing: a b)");
            var before = LinoDatabaseOutput.FormatDatabase(links).ToArray();
            var multilineName = $"first{newline}second";

            var error = Assert.Throws<FormatException>(() => LinoDatabaseInput.ImportText(
                links, $"(a: b a){newline}(new: '{multilineName}' 'C:\\Users\\'){newline}(c: ~1{{zz}} c)"));

            Assert.Contains("line 4", error.Message);
            Assert.Equal(before, LinoDatabaseOutput.FormatDatabase(links));
            Assert.Equal(links.Constants.Null, links.GetByName("new"));
            Assert.Equal(links.Constants.Null, links.GetByName(multilineName));
            Assert.Equal(links.Constants.Null, links.GetByName(@"C:\Users\"));
        });
    }

    [Fact]
    public void ImportText_ReproducesNumberedLinksAtTheirExplicitIndexes()
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, """
                (1: 1 1)
                (2: 1 2)
                (3: 2 1)
                """);

            Assert.Equal(new[]
            {
                "(1: 1 1)",
                "(2: 1 2)",
                "(3: 2 1)"
            }, LinoDatabaseOutput.FormatDatabase(links));
        });
    }

    [Fact]
    public void ImportText_CreatesNamedReferencesAsPointLinks()
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, "(child: father mother)");

            var father = links.GetByName("father");
            var mother = links.GetByName("mother");
            var child = links.GetByName("child");

            Assert.NotEqual(links.Constants.Null, father);
            Assert.NotEqual(links.Constants.Null, mother);
            Assert.NotEqual(links.Constants.Null, child);

            Assert.Equal(new Link<uint>(father, father, father), new Link<uint>(links.GetLink(father)));
            Assert.Equal(new Link<uint>(mother, mother, mother), new Link<uint>(links.GetLink(mother)));
            Assert.Equal(new Link<uint>(child, father, mother), new Link<uint>(links.GetLink(child)));

            Assert.Contains("(father: father father)", LinoDatabaseOutput.FormatDatabase(links));
            Assert.Contains("(mother: mother mother)", LinoDatabaseOutput.FormatDatabase(links));
            Assert.Contains("(child: father mother)", LinoDatabaseOutput.FormatDatabase(links));
        });
    }

    [Fact]
    public void ImportText_TreatsOutOfRangeNumbersAsNames()
    {
        WithNamedLinks(links =>
        {
            var outOfRangeNumber = ((ulong)uint.MaxValue + 1UL).ToString();

            LinoDatabaseInput.ImportText(links, $"(child: \"{outOfRangeNumber}\" 2)");

            var child = links.GetByName("child");
            var formattedLinks = string.Join(", ", LinoDatabaseOutput.FormatDatabase(links));

            Assert.True(child != links.Constants.Null, formattedLinks);
            var childLink = new Link<uint>(links.GetLink(child));
            var numericName = childLink.Source;

            Assert.Equal(outOfRangeNumber, links.GetName(numericName));
            Assert.Equal(new Link<uint>(numericName, numericName, numericName), new Link<uint>(links.GetLink(numericName)));
        });
    }

    [Fact]
    public void ImportText_UnquotesNamesWrittenByExporter()
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, """
                ('source name': 'source name' 'source name')
                ('target:ref': 'target:ref' 'target:ref')
                ('child(ref)': 'source name' 'target:ref')
                """);

            var lines = LinoDatabaseOutput.FormatDatabase(links);

            Assert.Contains("('source name': 'source name' 'source name')", lines);
            Assert.Contains("('target:ref': 'target:ref' 'target:ref')", lines);
            Assert.Contains("('child(ref)': 'source name' 'target:ref')", lines);
        });
    }

    [Theory]
    [InlineData("'")]
    [InlineData("\"")]
    public void ImportText_KeepsQuotedNumbersDistinctFromAddresses(string quote)
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, $"(1: 1 1)\n({quote}1{quote}: {quote}1{quote} {quote}1{quote})\n(3: 1 {quote}1{quote})");

            Assert.Null(links.GetName(1));
            Assert.Equal("1", links.GetName(2));
            Assert.Equal(new Link<uint>(2, 2, 2), new Link<uint>(links.GetLink(2u)));
            Assert.Equal(new Link<uint>(3, 1, 2), new Link<uint>(links.GetLink(3u)));
        });
    }

    [Theory]
    [InlineData("'", @"x\y")]
    [InlineData("\"", @"x\y")]
    [InlineData("'", @"C:\Users\name\")]
    [InlineData("\"", @"\\server\share\")]
    [InlineData("'", @"literal\n\r\t")]
    [InlineData("'", "first\nsecond")]
    [InlineData("\"", "first\r\nsecond")]
    [InlineData("`", "both'\"quotes")]
    public void ImportText_PreservesLiteralQuotedNames(string quote, string name)
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, $"({quote}{name}{quote}: {quote}{name}{quote} {quote}{name}{quote})");

            var index = links.GetByName(name);
            Assert.NotEqual(links.Constants.Null, index);
            Assert.Equal(name, links.GetName(index));
            Assert.Equal(new Link<uint>(index, index, index), new Link<uint>(links.GetLink(index)));
        });
    }

    [Fact]
    public void ImportText_PreservesBackslashInIssueReproduction()
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, @"(a: a 'x\y')");

            var a = links.GetByName("a");
            var target = new Link<uint>(links.GetLink(a)).Target;
            Assert.Equal(@"x\y", links.GetName(target));
            Assert.Equal(links.Constants.Null, links.GetByName("xy"));
        });
    }

    [Theory]
    [InlineData("~1{31}", "1")]
    [InlineData("~1{610d0a62}", "a\r\nb")]
    [InlineData("~1{626f746827226071756f746573}", "both'\"`quotes")]
    public void ImportText_DecodesLosslessLiteralsAsNames(string literal, string name)
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, $"(1: 1 1)\n({literal}: {literal} {literal})");

            Assert.Null(links.GetName(1));
            Assert.Equal(name, links.GetName(2));
            Assert.Equal(new Link<uint>(2, 2, 2), new Link<uint>(links.GetLink(2u)));
        });
    }

    [Theory]
    [InlineData("~1{0}")]
    [InlineData("~1{zz}")]
    [InlineData("~2{61}")]
    [InlineData("~1{ff}")]
    [InlineData("~1{61")]
    public void ImportText_RejectsInvalidReferenceLiterals(string literal)
    {
        WithNamedLinks(links =>
        {
            var error = Assert.Throws<FormatException>(() =>
                LinoDatabaseInput.ImportText(links, $"\n\n({literal}: a b)"));

            Assert.Contains("Invalid LiNo import line 3:", error.Message);
        });
    }

    [Theory]
    [InlineData("(a: a)")]
    [InlineData("(a: a a a)")]
    [InlineData("(a: a a) trailing")]
    [InlineData("(a: a a) (b: b b)")]
    [InlineData("(a: a\na)")]
    [InlineData("(a: a 'unterminated)")]
    public void ImportText_StillRejectsMalformedDefinitions(string text)
    {
        WithNamedLinks(links => Assert.Throws<FormatException>(() => LinoDatabaseInput.ImportText(links, text)));
    }

    [Fact]
    public void ImportText_SeparatesDefinitionsAroundMultilineNames()
    {
        WithNamedLinks(links =>
        {
            LinoDatabaseInput.ImportText(links, "\r\n \t(a: 'first\r\nsecond' a) \r\n\r\n(b: a '~1{31}')\r\n");

            var a = links.GetByName("a");
            var b = links.GetByName("b");
            var first = new Link<uint>(links.GetLink(a));
            var second = new Link<uint>(links.GetLink(b));
            Assert.Equal("first\r\nsecond", links.GetName(first.Source));
            Assert.Equal(a, first.Target);
            Assert.Equal(a, second.Source);
            Assert.Equal("~1{31}", links.GetName(second.Target));
        });
    }

    private static void WithNamedLinks(Action<NamedTypesDecorator<uint>> test)
    {
        var dbPath = Path.GetTempFileName();
        var namesDbPath = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dbPath);

        try
        {
            using var links = new NamedTypesDecorator<uint>(dbPath, false);
            test(links);
        }
        finally
        {
            if (File.Exists(dbPath))
            {
                File.Delete(dbPath);
            }
            if (File.Exists(namesDbPath))
            {
                File.Delete(namesDbPath);
            }
        }
    }
}
