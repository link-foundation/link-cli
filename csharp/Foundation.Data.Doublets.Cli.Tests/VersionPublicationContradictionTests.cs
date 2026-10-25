using System.Text.Json;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionPublicationContradictionTests
{
    private static IEnumerable<(uint Id, string Name)> Markers(NamedTypesDecorator<uint> store)
    {
        var any = store.Constants.Any;
        return store.All(new Link<uint>(any, any, any)).Where(row => row is not null)
            .Select(row => (row![0], store.GetName(row[0])!)).Where(row => row.Item2 is not null);
    }

    private static void Write(NamedTypesDecorator<uint> store, string name) => store.SetName(store.CreateAndUpdate(0u, 0u), name);

    [Theory]
    [InlineData("unknown-branch")]
    [InlineData("wrong-current")]
    [InlineData("head-ahead")]
    [InlineData("applied-ahead")]
    [InlineData("foreign-record")]
    [InlineData("missing-record")]
    [InlineData("foreign-attribution")]
    [InlineData("multiple-intents")]
    [InlineData("conflicting-decisions")]
    public void ContradictoryIntentPreservesSidecarAndCommittedRows(string scenario)
    {
        var directory = Directory.CreateTempSubdirectory("version-contradiction-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            var path = Path.Combine(directory, "branches.links");
            using var branches = new NamedTypesDecorator<uint>(path);
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            var id = vc.CreateAndUpdate(11u, 12u);
            var original = vc.CurrentSequence;
            vc.Branch(VersionExitProbe.Topic); vc.SwitchBranch(VersionExitProbe.Topic);
            vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
            var attempted = vc.CurrentSequence;
            var intent = new VersionControlDecorator.PublicationIntent(tx.Log.Last().TransactionId, original, attempted, VersionExitProbe.Topic);
            var markers = Markers(branches).ToArray();
            void Replace(string prefix, string value) => branches.SetName(markers.Single(row => row.Name.StartsWith(prefix, StringComparison.Ordinal)).Id, value);
            switch (scenario)
            {
                case "unknown-branch": intent = intent with { Branch = "unknown" }; break;
                case "wrong-current": Replace("__vc:current=", "__vc:current=main"); break;
                case "head-ahead":
                    var branch = vc.ListBranches().Single(b => b.Name == VersionExitProbe.Topic);
                    var oldMarker = VersionControlDecorator.EncodeBranchMarker(branch);
                    branches.SetName(branches.GetByName(oldMarker), VersionControlDecorator.EncodeBranchMarker(branch with { Head = attempted + 1 }));
                    break;
                case "applied-ahead": Replace("__vc:applied=", $"__vc:applied={attempted + 1}"); break;
                case "foreign-record": intent = intent with { Before = 0 }; break;
                case "missing-record": intent = intent with { After = attempted + 1 }; break;
                case "foreign-attribution": Replace($"__vc:trans:{attempted}:", $"__vc:trans:{attempted}:branch=main"); break;
                case "multiple-intents": Write(branches, VersionControlDecorator.EncodePublicationMarker(intent with { Id = Guid.NewGuid() })); break;
                case "conflicting-decisions": Write(log, "__transactions:rollback:" + intent.Id.ToString("N")); tx.Recover(); break;
            }
            Write(branches, VersionControlDecorator.EncodePublicationMarker(intent));
            branches.Flush(); log.Flush();
            var namesPath = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(path);
            var before = File.ReadAllBytes(path); var namesBefore = File.ReadAllBytes(namesPath);
            Assert.Throws<InvalidDataException>(() => new VersionControlDecorator(tx, branches));
            branches.Flush();
            Assert.Equal(before, File.ReadAllBytes(path));
            Assert.Equal(namesBefore, File.ReadAllBytes(namesPath));
            Assert.Equal(21u, data.GetSource(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void SharedIntentVectorsResolveAbortsAndPreserveMalformedFiles()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "version-publication-vectors.json")));
        foreach (var valid in new[] { true, false })
            foreach (var row in vectors.RootElement.GetProperty(valid ? "intents" : "invalidIntents").EnumerateArray())
            {
                var marker = valid ? row.GetProperty("marker").GetString()! : row.GetString()!;
                if (!marker.StartsWith("__vc:pending:", StringComparison.Ordinal)) continue;
                var branch = valid ? row.GetProperty("branch").GetString()! : "main";
                var directory = Directory.CreateTempSubdirectory("version-intent-vector-").FullName;
                try
                {
                    using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                    using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                    var path = Path.Combine(directory, "branches.links");
                    using var branches = new NamedTypesDecorator<uint>(path);
                    Write(branches, VersionControlDecorator.EncodeBranchMarker(new BranchInfo("main", null, 0, 0)));
                    if (branch != "main") Write(branches, VersionControlDecorator.EncodeBranchMarker(new BranchInfo(branch, "main", 0, 0)));
                    Write(branches, "__vc:current=" + branch); Write(branches, "__vc:applied=0"); Write(branches, marker);
                    branches.Flush();
                    var namesPath = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(path);
                    var before = File.ReadAllBytes(path); var namesBefore = File.ReadAllBytes(namesPath);
                    using var tx = new TransactionsDecorator(data, log);
                    if (valid)
                    {
                        using var vc = new VersionControlDecorator(tx, branches);
                        Assert.Equal(branch, vc.CurrentBranch); Assert.Equal(0, vc.CurrentSequence);
                        vc.Branch("following"); vc.Recover();
                        Assert.DoesNotContain(Markers(branches), item => item.Name.StartsWith("__vc:pending:", StringComparison.Ordinal));
                    }
                    else
                    {
                        Assert.Throws<InvalidDataException>(() => new VersionControlDecorator(tx, branches));
                        branches.Flush();
                        Assert.Equal(before, File.ReadAllBytes(path)); Assert.Equal(namesBefore, File.ReadAllBytes(namesPath));
                    }
                }
                finally { Directory.Delete(directory, recursive: true); }
            }
    }
}
