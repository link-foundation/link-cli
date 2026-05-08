using Foundation.Data.Doublets.Cli;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class LinoDatabaseOutputTests
{
    [Fact]
    public void FormatDatabase_UsesNumberedReferences_WhenLinksHaveNoNames()
    {
        WithNamedLinks(links =>
        {
            links.GetOrCreate(1u, 1u);
            links.GetOrCreate(1u, 2u);

            var lines = LinoDatabaseOutput.FormatDatabase(links);

            Assert.Equal(new[] { "(1: 1 1)", "(2: 1 2)" }, lines);
        });
    }

    [Fact]
    public void FormatDatabase_UsesNamesForIndexesSourcesAndTargets_WhenNamesExist()
    {
        WithNamedLinks(links =>
        {
            var father = links.GetOrCreate(1u, 1u);
            links.SetName(father, "father");
            var mother = links.GetOrCreate(2u, 2u);
            links.SetName(mother, "mother");
            var child = links.GetOrCreate(father, mother);
            links.SetName(child, "child");

            var lines = LinoDatabaseOutput.FormatDatabase(links);

            Assert.Equal(new[]
            {
                "(father: father father)",
                "(mother: mother mother)",
                "(child: father mother)"
            }, lines);
        });
    }

    [Fact]
    public void FormatDatabase_EscapesNamesThatNeedQuoting()
    {
        WithNamedLinks(links =>
        {
            var source = links.GetOrCreate(1u, 1u);
            links.SetName(source, "source name");
            var target = links.GetOrCreate(2u, 2u);
            links.SetName(target, "target:ref");
            var child = links.GetOrCreate(source, target);
            links.SetName(child, "child(ref)");

            var lines = LinoDatabaseOutput.FormatDatabase(links);

            Assert.Equal(new[]
            {
                "('source name': 'source name' 'source name')",
                "('target:ref': 'target:ref' 'target:ref')",
                "('child(ref)': 'source name' 'target:ref')"
            }, lines);
        });
    }

    [Fact]
    public void FormatDatabase_SelectsQuoteStyleForNamesContainingQuotes()
    {
        WithNamedLinks(links =>
        {
            var singleQuote = links.GetOrCreate(1u, 1u);
            links.SetName(singleQuote, "single'quote");
            var doubleQuote = links.GetOrCreate(2u, 2u);
            links.SetName(doubleQuote, "double\"quote");
            var bothQuotes = links.GetOrCreate(singleQuote, doubleQuote);
            links.SetName(bothQuotes, "both'\"quote");

            var lines = LinoDatabaseOutput.FormatDatabase(links);

            Assert.Equal(new[]
            {
                "(\"single'quote\": \"single'quote\" \"single'quote\")",
                "('double\"quote': 'double\"quote' 'double\"quote')",
                "('both\\'\"quote': \"single'quote\" 'double\"quote')"
            }, lines);
        });
    }

    [Fact]
    public void WriteToFile_WritesCompleteDatabaseAsLinoLines()
    {
        WithNamedLinks(links =>
        {
            links.GetOrCreate(1u, 1u);
            links.GetOrCreate(2u, 2u);

            var outputPath = Path.Combine(Path.GetTempPath(), $"{Guid.NewGuid():N}.lino");
            try
            {
                LinoDatabaseOutput.WriteToFile(links, outputPath);

                Assert.Equal(new[] { "(1: 1 1)", "(2: 2 2)" }, File.ReadAllLines(outputPath));
            }
            finally
            {
                if (File.Exists(outputPath))
                {
                    File.Delete(outputPath);
                }
            }
        });
    }

    [Fact]
    public void FormatStructure_RendersLeftBranchWithLinkIndexes()
    {
        WithNamedLinks(links =>
        {
            var first = links.GetOrCreate(0u, 0u);
            var second = links.GetOrCreate(first, first);
            var third = links.GetOrCreate(second, first);
            var fourth = links.GetOrCreate(third, second);

            var formatted = LinoDatabaseOutput.FormatStructure(links, fourth);

            Assert.Equal("(4: (3: (2: (1: 0 0) 1) 1) 2)", formatted);
        });
    }

    [Fact]
    public void FormatStructure_RendersRepeatedSourceAndTargetAsReferenceOnRight()
    {
        WithNamedLinks(links =>
        {
            var first = links.GetOrCreate(0u, 0u);
            var second = links.GetOrCreate(first, first);
            links.GetOrCreate(second, first);
            var fourth = links.GetOrCreate(second, second);

            var formatted = LinoDatabaseOutput.FormatStructure(links, fourth);

            Assert.Equal("(4: (2: (1: 0 0) 1) 2)", formatted);
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
