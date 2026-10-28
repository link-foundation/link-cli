using Platform.Data;
using Platform.Data.Doublets;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests;

public class StructureOutputTests
{
    private sealed class ObservedNames(string path) : NamedTypesDecorator<uint>(path)
    {
        public readonly List<uint> Reads = new();
        public int? FailAt;
        public override string? GetName(uint address)
        {
            Reads.Add(address);
            if (FailAt == Reads.Count - 1) throw new IOException($"name fault {FailAt}");
            return base.GetName(address);
        }
    }

    private static string Reference(INamedTypesLinks<uint> links, uint id) => LinoDatabaseOutput.Namify(links, id.ToString());

    private static string Oracle(INamedTypesLinks<uint> links, uint id, HashSet<uint> path)
    {
        if (!links.Exists(id)) throw new InvalidOperationException($"Link '{id}' does not exist.");
        if (!path.Add(id)) return Reference(links, id);
        try
        {
            var link = new DoubletLink(links.GetLink(id));
            var source = links.Exists(link.Source) && !path.Contains(link.Source)
                ? Oracle(links, link.Source, path) : Reference(links, link.Source);
            var target = Reference(links, link.Target);
            return $"({Reference(links, link.Index)}: {source} {target})";
        }
        finally { path.Remove(id); }
    }

    private static void WithLinks(Action<ObservedNames> action)
    {
        var directory = Directory.CreateTempSubdirectory("structure-output-").FullName;
        try
        {
            using var links = new ObservedNames(Path.Combine(directory, "data.links"));
            action(links);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void FiniteGraphsMatchIndependentSourceSpineOracle()
    {
        WithLinks(links =>
        {
            var ids = new List<uint>();
            var names = new[] { "", "snow 雪", "both'\"quotes", "line\nzero\0", "42", "plain", "a:b", "~1{31}" };
            for (uint index = 0; index < 24; index++)
            {
                var source = index == 0 ? 0u : ids[(int)((index * 7 + index / 3) % index)];
                var target = index % 3 == 0 ? source : 70u + index;
                var id = links.GetOrCreate(source, target);
                ids.Add(id);
                links.SetName(id, names[index % names.Length] + (index == 0 ? "" : index));
            }
            foreach (var root in ids)
                Assert.Equal(Oracle(links, root, new()), LinoDatabaseOutput.FormatStructure(links, root));
        });
    }

    [Fact]
    public void CyclesSharedChildrenAndMissingRootsPreserveOutput()
    {
        WithLinks(links =>
        {
            var a = links.Create(); var b = links.Create();
            links.Update(a, b, b); links.Update(b, a, a);
            links.SetName(a, "a 雪"); links.SetName(b, "both'\"quotes");
            foreach (var root in new[] { a, b })
                Assert.Equal(Oracle(links, root, new()), LinoDatabaseOutput.FormatStructure(links, root));
            var self = links.Create(); links.Update(self, self, self);
            Assert.Equal(Oracle(links, self, new()), LinoDatabaseOutput.FormatStructure(links, self));
            Assert.Throws<InvalidOperationException>(() => LinoDatabaseOutput.FormatStructure(links, 123456u));
        });
    }

    [Fact]
    public void NameFailuresPreserveLookupOrderAndFollowingReads()
    {
        WithLinks(links =>
        {
            var first = links.GetOrCreate(0u, 70u);
            var second = links.GetOrCreate(first, 71u);
            var root = links.GetOrCreate(second, 72u);
            for (int position = 0; position <= 7; position++)
            {
                links.Reads.Clear(); links.FailAt = position;
                string? expected = null;
                var expectedError = Record.Exception(() => expected = Oracle(links, root, new()));
                var expectedReads = links.Reads.ToArray();
                links.Reads.Clear();
                string? actual = null;
                var actualError = Record.Exception(() => actual = LinoDatabaseOutput.FormatStructure(links, root));
                Assert.Equal(expectedError?.Message, actualError?.Message);
                Assert.Equal(expected, actual);
                Assert.Equal(expectedReads, links.Reads);
                links.FailAt = null;
                Assert.Equal(Oracle(links, root, new()), LinoDatabaseOutput.FormatStructure(links, root));
            }
        });
    }

    [Fact]
    public void PersistentOutputMatchesAfterRepeatedReopening()
    {
        var directory = Directory.CreateTempSubdirectory("structure-reopen-").FullName;
        var path = Path.Combine(directory, "data.links");
        uint root;
        string expected;
        try
        {
            using (var links = new NamedTypesDecorator<uint>(path))
            {
                var first = links.GetOrCreate(0u, 0u); links.SetName(first, "leaf 雪");
                root = links.GetOrCreate(first, first); links.SetName(root, "both'\"quotes");
                expected = Oracle(links, root, new());
            }
            for (int reopen = 0; reopen < 2; reopen++)
            {
                using var links = new NamedTypesDecorator<uint>(path);
                Assert.Equal(expected, LinoDatabaseOutput.FormatStructure(links, root));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
