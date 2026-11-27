namespace Foundation.Data.Doublets.Cli.Tests;

public class CliImportAtomicityTests
{
    [Fact]
    public async Task ImportDepth_DefaultsTo64AndAllowsABoundedOverride()
    {
        var directory = Directory.CreateTempSubdirectory("clink-import-depth-").FullName;
        try
        {
            var database = Path.Combine(directory, "data.links");
            var input = Path.Combine(directory, "input.lino");
            var exported = Path.Combine(directory, "exported.lino");
            string Document(int depth) =>
                $"(root: end {string.Concat(Enumerable.Repeat("(item ", depth - 1))}end{new string(')', depth)}";

            await File.WriteAllTextAsync(input, Document(64));
            Clink.AssertSucceeded(await Clink.RunAsync("--db", database, "--in", input));

            foreach (var (depth, limit, options) in new (int, int, string[])[]
            {
                (65, 64, ["--import"]),
                (129, 128, ["--in-max-depth=128", "--lino-input"]),
                (2, 1, ["--in-max-depth", "1", "--in"]),
            })
            {
                await File.WriteAllTextAsync(input, $"(unwritten: fresh another)\n{Document(depth)}");
                var failure = await Clink.RunAsync(["--db", database, .. options, input]);
                Assert.True(failure.ExitCode == 1, failure.Stderr);
                Assert.Contains($"nesting depth exceeds the maximum of {limit}", failure.Stderr);
            }

            foreach (var invalid in new[] { "0", "129", "-1", "1.5", "abc" })
            {
                var failure = await Clink.RunAsync("--db", database, "--in", input, $"--in-max-depth={invalid}");
                Assert.True(failure.ExitCode == 1, failure.Stderr);
                Assert.Contains("--in-max-depth", failure.Stderr);
            }

            await File.WriteAllTextAsync(input, Document(128));
            Clink.AssertSucceeded(await Clink.RunAsync(
                "--db", database, "--in-max-depth", "128", "--lino-input", input, "--out", exported));
            var rows = await File.ReadAllLinesAsync(exported);
            Assert.Contains(rows, line => line.StartsWith("(root: end ", StringComparison.Ordinal));
            Assert.DoesNotContain(rows, line => line.Contains("unwritten"));
            Assert.Equal(130, rows.Length);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

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
