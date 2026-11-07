using System.Reflection;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionAncestryOracleTests
{
    [Fact(Timeout = 30000)]
    public async Task PathsMatchIndependentShallowRecursiveReference()
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("ancestry-oracle-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var metadata = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, metadata);
            // Reflection supplies finite in-memory cases only. The independent
            // public tests cover actual metadata, replay and repeated reopening.
            const BindingFlags flags = BindingFlags.Instance | BindingFlags.NonPublic;
            var branches = (Dictionary<string, BranchInfo>)typeof(VersionControlDecorator).GetField("_branches", flags)!.GetValue(vc)!;
            var attribution = (Dictionary<long, string>)typeof(VersionControlDecorator).GetField("_transitionBranches", flags)!.GetValue(vc)!;
            var build = typeof(VersionControlDecorator).GetMethod("BuildBranchSeqsLocked", flags)!;
            var names = new[] { "root", "middle", "leaf", "unrelated", "missing" };
            ulong state = 0x138;
            ulong Next() { state = unchecked(state * 6364136223846793005 + 1); return state >> 32; }
            List<long> Reference(string name, HashSet<string> visited)
            {
                if (!branches.TryGetValue(name, out var info) || !visited.Add(name)) return new();
                var inherited = info.Parent is null ? new List<long>() : Reference(info.Parent, visited);
                inherited.RemoveAll(sequence => sequence > info.ForkSeq);
                inherited.AddRange(attribution.Where(pair => pair.Value == name && pair.Key <= info.Head).Select(pair => pair.Key).Order());
                return inherited;
            }
            for (var sample = 0; sample < 512; ++sample)
            {
                branches.Clear(); attribution.Clear();
                foreach (var (name, usualParent) in new (string, string?)[] { ("root", null), ("middle", "root"), ("leaf", "middle"), ("unrelated", null) })
                {
                    var parent = (sample % 4, name) switch { (1, "root") => "leaf", (2, "middle") => "missing", _ => usualParent };
                    branches[name] = new(name, parent, (long)(Next() % 14), (long)(Next() % 14));
                }
                // Reverse insertion ensures dictionary order is not sequence order.
                for (var sequence = 12L; sequence >= 1; --sequence)
                    attribution[sequence] = names[(int)(Next() % (ulong)names.Length)];
                foreach (var target in names)
                    Assert.Equal(Reference(target, new(StringComparer.Ordinal)), (List<long>)build.Invoke(vc, [target])!);
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
