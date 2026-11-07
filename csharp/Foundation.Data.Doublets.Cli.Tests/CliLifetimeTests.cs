using System.Reflection;

namespace Foundation.Data.Doublets.Cli.Tests;

[Collection("Diagnostic console")]
public class CliLifetimeTests
{
    [Theory]
    [InlineData("cache")]
    [InlineData("transactions")]
    [InlineData("version")]
    [InlineData("triggers")]
    [InlineData("construction")]
    [InlineData("version-construction")]
    [InlineData("trigger-construction")]
    [InlineData("async")]
    [InlineData("version-list")]
    [InlineData("embedded")]
    public void CommandReleasesOwnedFilesBeforeReturning(string scenario)
    {
        var directory = Directory.CreateTempSubdirectory("cli-lifetime-").FullName;
        var data = Path.Combine(directory, "data.links");
        var arguments = new List<string> { "--db", data };
        var expected = 0;
        switch (scenario)
        {
            case "transactions": arguments.AddRange(["--transactions", "((("]); expected = 1; break;
            case "version": arguments.AddRange(["--checkout", "missing-tag"]); expected = 1; break;
            case "triggers": arguments.AddRange(["--once", "() ((1 1))"]); break;
            case "async": arguments.AddRange(["--transactions", "--commit-mode", "async", "() ((1 1))"]); break;
            case "version-list": arguments.Add("--list-branches"); break;
            case "embedded": arguments.AddRange(["--embed-triggers", "--once", "() ((1 1))"]); break;
            case "construction":
            case "version-construction":
            case "trigger-construction":
                var blocked = Path.Combine(directory, "blocked.links");
                Directory.CreateDirectory(blocked);
                var option = scenario switch
                {
                    "version-construction" => "--vc-file",
                    "trigger-construction" => "--triggers-file",
                    _ => "--transactions-file"
                };
                arguments.AddRange([option, blocked]); expected = 1; break;
        }
        var stdout = Console.Out; var stderr = Console.Error;
        using var output = new StringWriter(); using var errors = new StringWriter();
        try
        {
            Console.SetOut(output); Console.SetError(errors);
            var entry = Assembly.Load("clink").EntryPoint!;
            Assert.Equal(expected, Assert.IsType<int>(entry.Invoke(null, new object[] { arguments.ToArray() })));
            var files = Directory.GetFiles(directory, "*.links");
            Assert.NotEmpty(files);
            foreach (var file in files)
            {
                using var exclusive = new FileStream(file, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
                Assert.True(exclusive.CanWrite);
            }
        }
        finally
        {
            Console.SetOut(stdout); Console.SetError(stderr);
            // Keep failing pre-fix probes finite and release leaked finalizable
            // handles before removing their temporary directory on Windows.
            GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect();
            Directory.Delete(directory, recursive: true);
        }
    }
}
