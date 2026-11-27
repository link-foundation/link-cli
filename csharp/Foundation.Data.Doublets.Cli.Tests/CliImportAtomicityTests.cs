using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CliImportAtomicityTests
{
    [Theory]
    [InlineData("sync")]
    [InlineData("async")]
    public async Task TransactionalImportAndQueryEachCommitOnce(string mode)
    {
        var directory = Directory.CreateTempSubdirectory("clink-batch-").FullName;
        try
        {
            var db = Path.Combine(directory, "data.links");
            var input = Path.Combine(directory, "input.lino");
            var rows = Enumerable.Range(1, 10).Select(i => $"(k{i}: a{i} b{i})").ToArray();
            await File.WriteAllLinesAsync(input, rows);
            string[] args = ["--db", db, "--transactions", "--commit-mode", mode, "--trace", "--in", input];

            var import = await Clink.RunAsync(args);
            Clink.AssertSucceeded(import);
            var commit = Assert.Single(import.Stdout.Split('\n'), line => line.Contains("Committed tx "));
            var lastSequence = commit.Split("transitions=")[1].Split(')')[0];

            var journal = Path.Combine(directory, "data.transitions.log");
            Assert.Equal("__transactions:applied:" + lastSequence, File.ReadLines(journal).Last());
            var before = await File.ReadAllBytesAsync(journal);
            Clink.AssertSucceeded(await Clink.RunAsync(args));
            Assert.Equal(before, await File.ReadAllBytesAsync(journal));

            rows[0] = "(k1: b1 a1)";
            await File.WriteAllLinesAsync(input, rows);
            var query = await Clink.RunAsync([.. args, "() ((31 31) (32 32))"]);
            Clink.AssertSucceeded(query);
            Assert.Single(query.Stdout.Split('\n'), line => line.Contains("Committed tx "));
            var readback = await Clink.RunAsync("--db", db, "--transactions", "--after");
            Clink.AssertSucceeded(readback);
            foreach (var row in rows.Concat(["(31: 31 31)", "(32: 32 32)"]))
                Assert.Contains(row, readback.Stdout.Split('\n'));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task QueryErrorRollsBackImportInTheSameInvocation()
    {
        var directory = Directory.CreateTempSubdirectory("clink-batch-error-").FullName;
        try
        {
            var db = Path.Combine(directory, "data.links");
            var input = Path.Combine(directory, "input.lino");
            string[] args = ["--db", db, "--vc", "--in", input];
            await File.WriteAllTextAsync(input, "(base: base base)\n");
            Clink.AssertSucceeded(await Clink.RunAsync(args));
            var branches = await Clink.RunAsync("--db", db, "--list-branches");
            Clink.AssertSucceeded(branches);

            await File.WriteAllTextAsync(input, "(base: fresh base)\n(new: base fresh)\n");
            var failed = await Clink.RunAsync([.. args, "((("]);
            Assert.Equal(1, failed.ExitCode);
            Assert.Contains("Parse error", failed.Stderr);
            var readback = await Clink.RunAsync("--db", db, "--vc", "--after");
            Clink.AssertSucceeded(readback);
            Assert.Equal("(base: base base)\n", readback.Stdout);
            Assert.Equal(branches.Stdout, (await Clink.RunAsync("--db", db, "--list-branches")).Stdout);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task KillingAnImportBeforeItsCommitRecoversTheOriginalMappedStore()
    {
        if (OperatingSystem.IsWindows()) return; // The synchronization barrier is a POSIX FIFO.
        var directory = Directory.CreateTempSubdirectory("clink-batch-crash-").FullName;
        try
        {
            var db = Path.Combine(directory, "data.links");
            var input = Path.Combine(directory, "input.lino");
            var fifo = Path.Combine(directory, "export.fifo");
            var mkfifo = new ProcessStartInfo("mkfifo") { ArgumentList = { fifo } };
            using var createFifo = Process.Start(mkfifo)!;
            await createFifo.WaitForExitAsync();
            Assert.Equal(0, createFifo.ExitCode);
            string[] args = ["--db", db, "--transactions", "--in", input];
            await File.WriteAllTextAsync(input, "(base: base base)\n");
            Clink.AssertSucceeded(await Clink.RunAsync(args));
            await File.WriteAllTextAsync(input, "(base: added base)\n(new: base added)\n");

            using var child = Process.Start(Clink.StartInfo([.. args, "--after", "--out", fifo]))!;
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            var sawImport = false;
            try
            {
                while (await child.StandardOutput.ReadLineAsync(deadline.Token) is { } line)
                {
                    if (line != "(new: base added)") continue;
                    sawImport = true;
                    break;
                }
            }
            finally
            {
                if (!child.HasExited) child.Kill();
                await child.WaitForExitAsync();
            }
            Assert.True(sawImport, "The child did not reach the export FIFO.");

            var readback = await Clink.RunAsync("--db", db, "--transactions", "--after");
            Clink.AssertSucceeded(readback);
            Assert.Equal("(base: base base)\n", readback.Stdout);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

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
