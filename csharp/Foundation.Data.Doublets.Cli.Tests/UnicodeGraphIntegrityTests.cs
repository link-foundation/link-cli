using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli.Tests;

public class UnicodeGraphIntegrityTests
{
    private static uint Wrap(ILinks<uint> links, UnicodeStringStorage<uint> storage, uint root) =>
        links.GetOrCreate(storage.StringType, links.GetOrCreate(root, storage.UnicodeSequenceType));

    private static void WithStorage(string path, Action<ILinks<uint>, UnicodeStringStorage<uint>> action)
    {
        var constants = new LinksConstants<uint>(enableExternalReferencesSupport: true);
        using var memory = new FileMappedResizableDirectMemory(path, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
        using var raw = new UnitedMemoryLinks<uint>(memory, UnitedMemoryLinks<uint>.DefaultLinksSizeStep, constants, Platform.Data.Doublets.Memory.IndexTreeType.Default);
        var links = raw.DecorateWithAutomaticUniquenessAndUsagesRepointing();
        action(links, new UnicodeStringStorage<uint>(links));
    }

    private static void WithStorage(Action<ILinks<uint>, UnicodeStringStorage<uint>> action)
    {
        var directory = Directory.CreateTempSubdirectory("unicode-graph-").FullName;
        try { WithStorage(Path.Combine(directory, "data.links"), action); }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void SharedSubtreesExpandEveryOccurrenceInUnicodeOrder()
    {
        WithStorage((links, storage) =>
        {
            var original = storage.CreateString("A😀雪");
            var root = links.GetSource(links.GetTarget(original));
            var value = Wrap(links, storage, links.GetOrCreate(root, root));
            var wrapper = links.GetOrCreate(0u, links.GetOrCreate(0u, value));
            Assert.Equal("A😀雪A😀雪", storage.GetString(wrapper));
            Assert.Equal("A😀雪A😀雪", storage.UnicodeSequenceToStringConverter.Convert(links.GetTarget(value)));
            Assert.Throws<InvalidLinkFormatException>(() => storage.GetString(links.GetOrCreate(0u, wrapper)));
            Assert.Equal("", storage.GetString(storage.CreateString("")));
        });
    }

    [Fact]
    public void DeepFiniteSequenceUsesAnExplicitStack()
    {
        WithStorage((links, storage) =>
        {
            var symbol = links.GetSource(links.GetTarget(storage.CreateString("A")));
            var root = symbol;
            for (int i = 0; i < 2048; i++) root = links.GetOrCreate(root, symbol);
            Assert.Equal(new string('A', 2049), storage.GetString(Wrap(links, storage, root)));
        });
    }

    [Fact]
    public void MissingChildrenFailAndFollowingReadsRemainHealthy()
    {
        WithStorage((links, storage) =>
        {
            var symbol = links.GetSource(links.GetTarget(storage.CreateString("Z")));
            foreach (var root in new[] { links.GetOrCreate(0u, symbol), links.GetOrCreate(symbol, 0u) })
            {
                var value = Wrap(links, storage, root);
                for (int repeat = 0; repeat < 2; repeat++) Assert.Throws<InvalidDataException>(() => storage.GetString(value));
            }
            Assert.Equal("following 😀", storage.GetString(storage.CreateString("following 😀")));
            // Keep the existing C# UTF-16 string policy, including lone surrogates.
            Assert.Equal("\ud800", storage.GetString(storage.CreateString("\ud800")));
        });
    }

    [Fact]
    public void PersistedCycleRejectsStringAndNameOnRepeatedReopening()
    {
        var directory = Directory.CreateTempSubdirectory("unicode-graph-reopen-").FullName;
        var path = Path.Combine(directory, "data.links");
        uint value = 0, holder = 0;
        try
        {
            WithStorage(path, (links, storage) =>
            {
                var root = links.Create(); links.Update(root, root, root);
                value = Wrap(links, storage, root);
                holder = links.Create();
                links.GetOrCreate(holder, links.GetOrCreate(storage.NameType, value));
            });
            for (int reopen = 0; reopen < 2; reopen++)
            {
                WithStorage(path, (links, storage) =>
                {
                    var before = links.GetLink(value)!.ToArray();
                    Assert.Throws<InvalidDataException>(() => storage.GetString(value));
                    Assert.Throws<InvalidDataException>(() => storage.NamedLinks.GetName(holder));
                    Assert.Equal(before, links.GetLink(value));
                    Assert.Equal("following 雪", storage.GetString(storage.CreateString("following 雪")));
                });
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
