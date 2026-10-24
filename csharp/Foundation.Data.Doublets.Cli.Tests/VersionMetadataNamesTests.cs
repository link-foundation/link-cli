using Platform.Data;
using Platform.Data.Doublets;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMetadataNamesTests
{
    [Theory]
    [InlineData("topic:parent=decoy:fork=7:head=9/😀", "release=version/😀", false)]
    [InlineData("{\"name\":\"quoted\"}:parent=:", "\"quoted\"=value", false)]
    [InlineData("safe-branch", "release=version=😀", false)]
    [InlineData("zero\0line\n:parent=😀", "zero\0=\n", false)]
    [InlineData("legacy", "old=tag", true)]
    [InlineData("2|0041|-|0|0", "prefix=tag", true)]
    public void DelimiterAndUnicodeNamesSurviveBranchHistoryReopening(string branch, string tag, bool legacyMarkers)
    {
        var directory = Directory.CreateTempSubdirectory("version-names-").FullName;
        try
        {
            uint baseline, created;
            long baselineHead, head;
            var dataPath = Path.Combine(directory, "data.links");
            var logPath = Path.Combine(directory, "log.links");
            var branchPath = Path.Combine(directory, "branches.links");
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var metadata = new NamedTypesDecorator<uint>(branchPath))
            using (var tx = new TransactionsDecorator(data, log))
            using (var vc = new VersionControlDecorator(tx, metadata))
            {
                baseline = vc.CreateAndUpdate(11u, 12u); baselineHead = vc.CurrentSequence;
                vc.Branch(branch); vc.SwitchBranch(branch);
                created = vc.CreateAndUpdate(21u, 22u); head = vc.CurrentSequence;
                vc.Tag(tag);
                vc.Branch("child", head);
                vc.Tag("plain");
                if (legacyMarkers)
                {
                    foreach (var info in vc.ListBranches())
                    {
                        var link = metadata.GetByName(VersionControlDecorator.EncodeBranchMarker(info));
                        Assert.NotEqual(0u, link);
                        metadata.SetName(link, $"__vc:branch:{info.Name}:parent={info.Parent}:fork={info.ForkSeq}:head={info.Head}");
                    }
                }
            }
            using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
            using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
            using var recoveredMetadata = new NamedTypesDecorator<uint>(branchPath);
            using var recoveredTx = new TransactionsDecorator(recoveredData, recoveredLog);
            using var recovered = new VersionControlDecorator(recoveredTx, recoveredMetadata);
            Assert.Equal(branch, recovered.CurrentBranch);
            Assert.Equal(head, recovered.CurrentSequence);
            Assert.Contains(recovered.ListBranches(), value => value.Name == branch && value.Parent == "main" && value.Head == head);
            Assert.Contains(recovered.ListBranches(), value => value.Name == "child" && value.Parent == branch && value.ForkSeq == head);
            Assert.True(recovered.TryGetTag(tag, out var sequence)); Assert.Equal(head, sequence);
            Assert.True(recovered.TryGetTag("plain", out sequence)); Assert.Equal(head, sequence);
            recovered.SwitchBranch("main");
            Assert.Equal(baselineHead, recovered.CurrentSequence);
            Assert.True(recovered.Exists(baseline)); Assert.False(recovered.Exists(created));
            recovered.SwitchBranch("child");
            Assert.Equal(head, recovered.CurrentSequence); Assert.True(recovered.Exists(created));
            recovered.Checkout(baselineHead); Assert.False(recovered.Exists(created));
            recovered.Checkout(head); Assert.True(recovered.Exists(created));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(
        Path.Combine(AppContext.BaseDirectory, "Protocol", "version-metadata-vectors.json")));

    [Fact]
    public void SharedBranchWireVectorsPreserveExactNamesAndLegacyRecords()
    {
        using var vectors = Vectors();
        foreach (var value in vectors.RootElement.GetProperty("branches").EnumerateArray())
        {
            var expected = new BranchInfo(value.GetProperty("name").GetString()!,
                value.GetProperty("parent").GetString(), value.GetProperty("fork").GetInt64(),
                value.GetProperty("head").GetInt64());
            var marker = value.GetProperty("marker").GetString()!;
            Assert.True(VersionControlDecorator.TryDecodeBranchMarker(marker, out var actual));
            Assert.Equal(expected, actual);
            if (value.GetProperty("version").GetInt32() == 2)
                Assert.Equal(marker, VersionControlDecorator.EncodeBranchMarker(expected));
        }
        // A .NET string can also contain an unpaired code unit. Encoding must
        // not replace it as a UTF-8 conversion with fallback would do.
        var unpaired = new BranchInfo("\ud800:parent=", "\udfff", 0, 0);
        Assert.True(VersionControlDecorator.TryDecodeBranchMarker(
            VersionControlDecorator.EncodeBranchMarker(unpaired), out var decoded));
        Assert.Equal(unpaired, decoded);
    }

    [Fact]
    public void SharedMalformedBranchVectorsAreRejected()
    {
        using var vectors = Vectors();
        foreach (var marker in vectors.RootElement.GetProperty("invalidBranches").EnumerateArray())
            Assert.False(VersionControlDecorator.TryDecodeBranchMarker(marker.GetString()!, out _));
    }

    [Fact]
    public void SharedLegacyTagVectorsRecoverTheEntireName()
    {
        using var vectors = Vectors();
        var directory = Directory.CreateTempSubdirectory("version-tag-vectors-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var metadata = new NamedTypesDecorator<uint>(Path.Combine(directory, "metadata"));
            foreach (var value in vectors.RootElement.GetProperty("tags").EnumerateArray())
                metadata.SetName(metadata.CreateAndUpdate(0u, 0u), value.GetProperty("marker").GetString()!);
            foreach (var marker in vectors.RootElement.GetProperty("invalidTags").EnumerateArray())
                metadata.SetName(metadata.CreateAndUpdate(0u, 0u), marker.GetString()!);
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, metadata);
            Assert.Equal(vectors.RootElement.GetProperty("tags").GetArrayLength(), vc.ListTags().Count);
            foreach (var value in vectors.RootElement.GetProperty("tags").EnumerateArray())
            {
                Assert.True(vc.TryGetTag(value.GetProperty("name").GetString()!, out var sequence));
                Assert.Equal(value.GetProperty("sequence").GetInt64(), sequence);
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
