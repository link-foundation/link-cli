using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionExitSetupTests
{
    private sealed class ObservedStore(string path, List<string> calls) : NamedTypesDecorator<uint>(path)
    {
        public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            uint result = base.Create(substitution, handler);
            calls.Add("create-returned");
            return result;
        }
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            calls.Add("update-entered");
            return base.Update(restriction, substitution, handler);
        }
    }

    [Fact]
    public void InitialRowReportsCompletedCreationBeforeStartingItsUpdate()
    {
        string directory = Directory.CreateTempSubdirectory("version-setup-progress-").FullName;
        try
        {
            var calls = new List<string>();
            using var store = new ObservedStore(Path.Combine(directory, "data.links"), calls);
            uint id = VersionExitProbe.CreateInitialRow(store, calls.Add);
            Assert.Equal(11u, store.GetSource(id)); Assert.Equal(12u, store.GetTarget(id));
            Assert.Equal(new[] { "create-returned", "row-allocated", "update-entered", "row-created" }, calls);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void InitialRowKeepsTheCombinedHelpersDuplicateRowBehavior()
    {
        string directory = Directory.CreateTempSubdirectory("version-setup-unique-").FullName;
        try
        {
            using var reference = new NamedTypesDecorator<uint>(Path.Combine(directory, "reference.links"));
            using var store = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            uint existing = reference.CreateAndUpdate(11u, 12u);
            Assert.Equal(existing, store.CreateAndUpdate(11u, 12u));
            uint expected = reference.CreateAndUpdate(11u, 12u);
            uint result = VersionExitProbe.CreateInitialRow(store, _ => { });
            Assert.Equal(expected, result);
            Assert.Equal(reference.Count([reference.Constants.Any]), store.Count([store.Constants.Any]));
            foreach (uint address in new[] { existing, result })
            {
                Assert.Equal(reference.Exists(address), store.Exists(address));
                if (!reference.Exists(address)) continue;
                Assert.Equal(reference.GetSource(address), store.GetSource(address));
                Assert.Equal(reference.GetTarget(address), store.GetTarget(address));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

}
