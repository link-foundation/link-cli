using Foundation.Data.Doublets.Cli;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class LinoDatabaseInputTests
{
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

    private static void WithNamedLinks(Action<NamedTypesDecorator<uint>> test)
    {
        var dbPath = Path.GetTempFileName();
        var namesDbPath = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dbPath);

        try
        {
            var links = new NamedTypesDecorator<uint>(dbPath, false);
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
