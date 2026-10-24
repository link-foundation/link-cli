namespace Foundation.Data.Doublets.Cli.Tests;

public class CliQuotedQueryReferencesTests
{
    [Fact]
    public async Task QuotedQueryNamesRoundTripThroughExportAndQuery()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"clink-quoted-query-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var source = Path.Combine(directory, "source.links");
            var text = Path.Combine(directory, "export.lino");
            var replay = Path.Combine(directory, "replay.lino");
            var before = Path.Combine(directory, "before.bin");
            var after = Path.Combine(directory, "after.bin");
            Clink.AssertSucceeded(await Clink.RunAsync("--db", source, "() ((a: a a) (b: b b))"));
            Clink.AssertSucceeded(await Clink.RunAsync(
                "--db", source, "--auto-create-missing-references",
                "() ((zero: x \"0\") (one: x \"1\") (two: x '2') (large: x '99') (address: x 1))",
                "--out", text, "--export-binary", before));

            var exported = await File.ReadAllTextAsync(text);
            Assert.Contains("(zero: x '0')", exported);
            Assert.Contains("(one: x '1')", exported);
            Assert.Contains("(two: x '2')", exported);
            Assert.Contains("(large: x '99')", exported);
            Assert.Contains("(address: x a)", exported);
            Clink.AssertSucceeded(await Clink.RunAsync(
                "--db", Path.Combine(directory, "destination.links"),
                $"() ({exported})", "--out", replay, "--export-binary", after));
            Assert.Equal(exported, await File.ReadAllTextAsync(replay));
            Assert.Equal(await File.ReadAllBytesAsync(before), await File.ReadAllBytesAsync(after));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }
}
