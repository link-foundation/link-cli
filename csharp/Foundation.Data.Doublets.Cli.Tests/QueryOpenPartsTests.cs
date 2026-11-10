using Platform.Data;
using Platform.Data.Doublets;
using Processor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

namespace Foundation.Data.Doublets.Cli.Tests;

public class QueryOpenPartsTests
{
    [Theory]
    [InlineData("((1: 1 1)) ((1: 2 *))", 2u, 1u)]
    [InlineData("((1: 1 1)) ((1: * 2))", 1u, 2u)]
    [InlineData("((1: 1 1)) ((1: * *))", 1u, 1u)]
    [InlineData("((1: 1 1)) ((1: *))", 1u, 1u)]
    [InlineData("((1: 1 1)) ((1: 2))", 1u, 1u)]
    [InlineData("((1: 1 1)) ((1: ()))", 1u, 1u)]
    [InlineData("((1: * *)) ((1: 2 *))", 2u, 1u)]
    [InlineData("(($i: 1 1)) (($i: 2 *))", 2u, 1u)]
    [InlineData("() ((1: 2 *))", 2u, 1u)]
    [InlineData("() ((1: * 2))", 1u, 2u)]
    [InlineData("((2: 2 2)) ((2: 2 2) (1: 2 *))", 2u, 1u)]
    public void APartLeftOpenKeepsItsStoredValue(string query, uint source, uint target)
    {
        var path = Path.GetTempFileName();
        try
        {
            using var links = new NamedTypesDecorator<uint>(path);
            links.GetOrCreate(1u, 1u);
            links.GetOrCreate(2u, 2u);

            Processor.ProcessQuery(links, new Processor.Options { Query = query });

            var link = new Link<uint>(links.GetLink(1u));
            Assert.Equal((source, target), (link.Source, link.Target));
            Assert.Equal(2u, new Link<uint>(links.GetLink(2u)).Source);
        }
        finally
        {
            File.Delete(path);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(path));
        }
    }
}
