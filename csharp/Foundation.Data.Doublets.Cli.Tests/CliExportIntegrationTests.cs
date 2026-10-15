namespace Foundation.Data.Doublets.Cli.Tests;

public class CliExportIntegrationTests
{
    [Fact]
    public async Task ExportAlias_WritesNumberedReferences()
    {
        var tempDirectory = CreateTempDirectory();

        try
        {
            var dbPath = Path.Combine(tempDirectory, "numbered.links");
            var outputPath = Path.Combine(tempDirectory, "numbered.lino");

            var result = await Clink.RunAsync("--db", dbPath, "() ((1 1) (2 2))", "--export", outputPath);

            Clink.AssertSucceeded(result);
            Assert.Equal(new[] { "(1: 1 1)", "(2: 2 2)" }, File.ReadAllLines(outputPath));
        }
        finally
        {
            Directory.Delete(tempDirectory, recursive: true);
        }
    }

    [Fact]
    public async Task ExportAlias_WritesNamedReferences()
    {
        var tempDirectory = CreateTempDirectory();

        try
        {
            var dbPath = Path.Combine(tempDirectory, "named.links");
            var outputPath = Path.Combine(tempDirectory, "named.lino");

            var result = await Clink.RunAsync(
                "--db",
                dbPath,
                "--auto-create-missing-references",
                "() ((child: father mother))",
                "--export",
                outputPath);

            Clink.AssertSucceeded(result);
            Assert.Equal(
                new[] { "(father: father father)", "(mother: mother mother)", "(child: father mother)" },
                File.ReadAllLines(outputPath));
        }
        finally
        {
            Directory.Delete(tempDirectory, recursive: true);
        }
    }

    [Fact]
    public async Task StructureOption_RendersLeftBranchWithIndexes()
    {
        var tempDirectory = CreateTempDirectory();

        try
        {
            var dbPath = Path.Combine(tempDirectory, "structure.links");

            Clink.AssertSucceeded(await Clink.RunAsync("--db", dbPath, "() ((1: 1 1))"));
            Clink.AssertSucceeded(await Clink.RunAsync("--db", dbPath, "() ((2: 1 2))"));
            Clink.AssertSucceeded(await Clink.RunAsync("--db", dbPath, "() ((3: 2 1))"));
            Clink.AssertSucceeded(await Clink.RunAsync("--db", dbPath, "() ((4: 3 2))"));

            var result = await Clink.RunAsync("--db", dbPath, "--structure", "4");

            Clink.AssertSucceeded(result);
            Assert.Equal("(4: (3: (2: (1: 1 1) 2) 1) 2)\n", result.Stdout);
        }
        finally
        {
            Directory.Delete(tempDirectory, recursive: true);
        }
    }

    [Fact]
    public async Task ImportOption_ReadsNumberedLinoFile()
    {
        var tempDirectory = CreateTempDirectory();

        try
        {
            var dbPath = Path.Combine(tempDirectory, "imported.links");
            var inputPath = Path.Combine(tempDirectory, "input.lino");
            var outputPath = Path.Combine(tempDirectory, "output.lino");
            await File.WriteAllLinesAsync(inputPath, new[]
            {
                "(1: 1 1)",
                "(2: 1 2)",
                "(3: 2 1)"
            });

            var result = await Clink.RunAsync("--db", dbPath, "--import", inputPath, "--export", outputPath);

            Clink.AssertSucceeded(result);
            Assert.Equal(new[]
            {
                "(1: 1 1)",
                "(2: 1 2)",
                "(3: 2 1)"
            }, File.ReadAllLines(outputPath));
        }
        finally
        {
            Directory.Delete(tempDirectory, recursive: true);
        }
    }

    [Fact]
    public async Task AlwaysTriggerOption_StoresTriggerAndAppliesItOnLaterChange()
    {
        var tempDirectory = CreateTempDirectory();

        try
        {
            var dbPath = Path.Combine(tempDirectory, "triggered.links");
            var triggersPath = Path.Combine(tempDirectory, "triggers.links");
            var outputPath = Path.Combine(tempDirectory, "triggered.lino");

            Clink.AssertSucceeded(await Clink.RunAsync(
                "--db",
                dbPath,
                "--triggers-file",
                triggersPath,
                "--always",
                "(((1: 1 1)) ((1: 1 2)))"));

            var result = await Clink.RunAsync(
                "--db",
                dbPath,
                "--triggers-file",
                triggersPath,
                "--auto-create-missing-references",
                "() ((1: 1 1))",
                "--export",
                outputPath);

            Clink.AssertSucceeded(result);
            Assert.Equal(new[] { "(1: 1 2)", "(2: 2 2)" }, File.ReadAllLines(outputPath));
        }
        finally
        {
            Directory.Delete(tempDirectory, recursive: true);
        }
    }

    [Theory]
    [InlineData("1")]
    [InlineData("0")]
    [InlineData("42")]
    public async Task TextExport_RoundTripsNumericNamesThroughImport(string name)
    {
        var tempDirectory = CreateTempDirectory();
        try
        {
            var sourcePath = Path.Combine(tempDirectory, "source.links");
            using (var links = new Foundation.Data.Doublets.Cli.NamedTypesDecorator<uint>(sourcePath, false))
            {
                Foundation.Data.Doublets.Cli.LinoDatabaseInput.ImportText(links, "(1: 1 1)\n(2: 2 2)\n(3: 1 2)");
                links.SetName(2, name);
            }

            var textPath = Path.Combine(tempDirectory, "export.lino");
            var beforePath = Path.Combine(tempDirectory, "before.bin");
            var afterPath = Path.Combine(tempDirectory, "after.bin");
            Clink.AssertSucceeded(await Clink.RunAsync(
                "--db", sourcePath, "--out", textPath, "--export-binary", beforePath));
            Clink.AssertSucceeded(await Clink.RunAsync(
                "--db", Path.Combine(tempDirectory, "target.links"),
                "--import", textPath, "--export-binary", afterPath));

            Assert.Equal(File.ReadAllBytes(beforePath), File.ReadAllBytes(afterPath));
        }
        finally
        {
            Directory.Delete(tempDirectory, recursive: true);
        }
    }

    private static string CreateTempDirectory()
    {
        var tempDirectory = Path.Combine(Path.GetTempPath(), $"clink-export-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDirectory);
        return tempDirectory;
    }
}
