namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary><c>--export-binary</c> and <c>--import-binary</c> copy a whole store through a store archive.</summary>
public sealed class CliStoreArchiveTests : IDisposable
{
    private readonly string _directory = Directory.CreateTempSubdirectory("clink-archive-").FullName;

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private string PathOf(string name) => Path.Combine(_directory, name);

    [Theory]
    [InlineData("--export-binary", "--import-binary")]
    [InlineData("--binary-output", "--binary-input")]
    [InlineData("--binary-out", "--binary-in")]
    public async Task TheCliCopiesAStoreThroughAnArchive(string export, string import)
    {
        var archive = PathOf("store.bin");
        Clink.AssertSucceeded(await Clink.RunAsync(
            "--db", PathOf("source.links"),
            "--auto-create-missing-references",
            "() ((child: father mother) (2: 2 1))",
            export, archive,
            "--out", PathOf("source.lino")));

        Clink.AssertSucceeded(await Clink.RunAsync(
            "--db", PathOf("target.links"),
            import, archive,
            "--out", PathOf("target.lino")));

        var lino = File.ReadAllText(PathOf("target.lino"));
        Assert.Equal(File.ReadAllText(PathOf("source.lino")), lino);
        Assert.Contains("(child: father mother)", lino);
    }

    [Fact]
    public async Task TheCliImportsTheArchiveBeforeTheLinoFileAndTheQuery()
    {
        var archive = PathOf("store.bin");
        await File.WriteAllBytesAsync(archive, Convert.FromHexString(
            "12 02 20 01 24 01 02 01 01 03 03 01 03 13 02 21 02 30 01 ff ff 9f ff fd ff 17 ff fc 9f 9e".Replace(" ", "")));
        await File.WriteAllTextAsync(PathOf("more.lino"), "(2: 3 4)\n");

        var result = await Clink.RunAsync(
            "--db", PathOf("target.links"),
            "--import-binary", archive,
            "--in", PathOf("more.lino"),
            "((4: 1 3)) ((4: 4 1))",
            "--after");

        Clink.AssertSucceeded(result);
        Assert.Equal("(a: a a)\n(2: é ab)\n(é: é é)\n(ab: ab a)\n", result.Stdout);
    }

    [Fact]
    public async Task TheCliReportsAMalformedArchive()
    {
        var archive = PathOf("broken.bin");
        await File.WriteAllBytesAsync(archive, new byte[] { 0x10, 0x00 });

        var result = await Clink.RunAsync("--db", PathOf("target.links"), "--import-binary", archive);

        Assert.NotEqual(0, result.ExitCode);
        Assert.Contains("Error reading binary store archive", result.Stderr);
        Assert.Contains("ends before its names", result.Stderr);
    }
}
