using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>
/// Shared golden fixtures (issue #111): <c>parity/golden-fixtures.json</c> lists the
/// <c>docs/protocol</c> files that the C# and Rust suites must both pass. This test loads
/// every listed fixture, checks its recorded SHA-256 and parses each JSON fixture. With
/// <c>GOLDEN_FIXTURES_RECEIPT</c> set, it writes the hashes it loaded so CI can prove with
/// <c>node parity/check-parity.mjs --receipt csharp &lt;file&gt;</c> that this suite did not
/// skip the fixtures. <c>rust/tests/golden_fixtures_tests.rs</c> is the Rust counterpart.
/// </summary>
public sealed class GoldenFixturesTests
{
    private static string RepositoryRoot()
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            if (File.Exists(Path.Combine(directory.FullName, "parity", "golden-fixtures.json")))
            {
                return directory.FullName;
            }
        }
        throw new FileNotFoundException("parity/golden-fixtures.json");
    }

    private static string Sha256Hex(byte[] bytes) => Convert.ToHexStringLower(SHA256.HashData(bytes));

    /// <summary>Fixtures that no other C# test reads get a content check here.</summary>
    private static void CheckSpecific(string path, byte[] bytes)
    {
        if (path.EndsWith("graphql/deep-fresh.json.gz", StringComparison.Ordinal))
        {
            Assert.Equal(new byte[] { 0x1f, 0x8b }, bytes[..2]);
        }
        if (path.EndsWith("graphql/deepclient-operations.json", StringComparison.Ordinal))
        {
            // Entries are recorded operations, plus one transport descriptor
            // (protocol, websocket) that has no query.
            var operations = JsonNode.Parse(bytes)!.AsArray()
                .Where(operation => operation!["query"] is not null)
                .ToList();
            Assert.NotEmpty(operations);
            foreach (var operation in operations)
            {
                Assert.Equal(JsonValueKind.String, operation!["operationName"]!.GetValueKind());
                Assert.Equal(JsonValueKind.String, operation["query"]!.GetValueKind());
            }
        }
    }

    [Fact]
    public void CSharpSuiteLoadsEveryGoldenFixture()
    {
        var root = RepositoryRoot();
        var manifest = JsonNode.Parse(File.ReadAllText(Path.Combine(root, "parity", "golden-fixtures.json")))!;
        var fixtures = manifest["fixtures"]!.AsArray();
        Assert.NotEmpty(fixtures);
        var loaded = new JsonObject();
        foreach (var fixture in fixtures)
        {
            var path = fixture!["path"]!.GetValue<string>();
            var bytes = File.ReadAllBytes(Path.Combine(root, path));
            var actual = Sha256Hex(bytes);
            Assert.True(
                fixture["sha256"]!.GetValue<string>() == actual,
                $"{path} changed; review both suites, then run node parity/check-parity.mjs --update");
            if (path.EndsWith(".json", StringComparison.Ordinal))
            {
                using var _ = JsonDocument.Parse(bytes);
            }
            CheckSpecific(path, bytes);
            loaded[path] = actual;
        }
        var receipt = Environment.GetEnvironmentVariable("GOLDEN_FIXTURES_RECEIPT");
        if (!string.IsNullOrEmpty(receipt))
        {
            var body = new JsonObject { ["suite"] = "csharp", ["fixtures"] = loaded };
            File.WriteAllText(receipt, body.ToJsonString(new JsonSerializerOptions { WriteIndented = true }) + "\n");
        }
    }

    [Fact]
    public void HashesAreLowercaseHex()
    {
        Assert.Equal(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            Sha256Hex("abc"u8.ToArray()));
    }
}
