using System.Diagnostics;

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

            var result = await RunClinkAsync("--db", dbPath, "() ((1 1) (2 2))", "--export", outputPath);

            AssertClinkSucceeded(result);
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

            var result = await RunClinkAsync(
                "--db",
                dbPath,
                "--auto-create-missing-references",
                "() ((child: father mother))",
                "--export",
                outputPath);

            AssertClinkSucceeded(result);
            Assert.Equal(
                new[] { "(father: father father)", "(mother: mother mother)", "(child: father mother)" },
                File.ReadAllLines(outputPath));
        }
        finally
        {
            Directory.Delete(tempDirectory, recursive: true);
        }
    }

    private static async Task<CommandResult> RunClinkAsync(params string[] clinkArguments)
    {
        var csharpDirectory = FindCsharpDirectory();
        var projectPath = Path.Combine(csharpDirectory, "Foundation.Data.Doublets.Cli", "Foundation.Data.Doublets.Cli.csproj");

        using var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = "dotnet",
                WorkingDirectory = csharpDirectory,
                RedirectStandardOutput = true,
                RedirectStandardError = true
            }
        };

        process.StartInfo.ArgumentList.Add("run");
        process.StartInfo.ArgumentList.Add("--project");
        process.StartInfo.ArgumentList.Add(projectPath);
        process.StartInfo.ArgumentList.Add("--");
        foreach (var argument in clinkArguments)
        {
            process.StartInfo.ArgumentList.Add(argument);
        }

        process.Start();
        var stdout = process.StandardOutput.ReadToEndAsync();
        var stderr = process.StandardError.ReadToEndAsync();

        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(60));
        try
        {
            await process.WaitForExitAsync(cts.Token);
        }
        catch (OperationCanceledException)
        {
            process.Kill(entireProcessTree: true);
            throw new TimeoutException("Timed out while running the C# clink integration test.");
        }

        return new CommandResult(process.ExitCode, await stdout, await stderr);
    }

    private static void AssertClinkSucceeded(CommandResult result)
    {
        Assert.True(
            result.ExitCode == 0,
            $"clink exited with {result.ExitCode}\nstdout:\n{result.Stdout}\nstderr:\n{result.Stderr}");
    }

    private static string FindCsharpDirectory()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "Foundation.Data.Doublets.Cli.sln")))
            {
                return directory.FullName;
            }

            directory = directory.Parent;
        }

        throw new DirectoryNotFoundException("Could not find the csharp directory containing Foundation.Data.Doublets.Cli.sln.");
    }

    private static string CreateTempDirectory()
    {
        var tempDirectory = Path.Combine(Path.GetTempPath(), $"clink-export-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDirectory);
        return tempDirectory;
    }

    private sealed record CommandResult(int ExitCode, string Stdout, string Stderr);
}
