namespace Foundation.Data.Doublets.Cli.Tests;

public class CliImportAtomicityTests
{
    [Theory]
    [InlineData("--import", false, false)]
    [InlineData("--import", true, false)]
    [InlineData("--in", false, false)]
    [InlineData("--in", true, false)]
    [InlineData("--lino-input", false, false)]
    [InlineData("--lino-input", true, false)]
    [InlineData("--import", false, true)]
    [InlineData("--import", true, true)]
    public async Task InvalidLastLine_LeavesDatabaseUnchangedAfterReopening(string option, bool populated, bool transactions)
    {
        var directory = Directory.CreateTempSubdirectory("clink-atomic-import-").FullName;
        try
        {
            var database = Path.Combine(directory, "data.links");
            var input = Path.Combine(directory, "input.lino");
            var before = Path.Combine(directory, "before.bin");
            var after = Path.Combine(directory, "after.bin");
            if (populated)
            {
                await File.WriteAllTextAsync(input, "(a: a a)\n(b: b b)\n(existing: a b)\n");
                Clink.AssertSucceeded(await Clink.RunAsync("--db", database, "--import", input));
            }
            Clink.AssertSucceeded(await Clink.RunAsync("--db", database, "--export-binary", before));
            var prefix = populated ? "(a: b a)\n(new: fresh fresh)" : "(a: a a)\n(b: b b)";
            await File.WriteAllTextAsync(input, $"{prefix}\n(c: c\n");

            var arguments = new List<string> { "--db", database, option, input, "--auto-create-missing-references" };
            if (transactions)
            {
                arguments.Add("--transactions");
            }
            var result = await Clink.RunAsync(arguments.ToArray());

            Assert.Equal(1, result.ExitCode);
            Assert.Contains("Invalid LiNo import line 3: Expected reference.", result.Stderr);
            Clink.AssertSucceeded(await Clink.RunAsync("--db", database, "--export-binary", after));
            Assert.Equal(File.ReadAllBytes(before), File.ReadAllBytes(after));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }
}
