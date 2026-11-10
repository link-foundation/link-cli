// The small library types: their untyped views and the refusals ordinary use never meets.
using System.Collections;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli.Tests;

public class LibrarySmallEdgeTests
{
    private sealed class Store : IDisposable
    {
        private readonly HeapResizableDirectMemory _memory = new();
        public Store() => Links = new UnitedMemoryLinks<uint>(_memory);
        public UnitedMemoryLinks<uint> Links { get; }
        public void Dispose() => Links.Dispose();
    }

    private sealed class ToNull : IAddressMap<uint>
    {
        public bool TryToInner(uint outer, out uint inner) { inner = outer; return true; }
        public uint ToOuter(uint inner) => 0;
    }

    private static void WithNamed(Action<NamedTypesDecorator<uint>> use)
    {
        var directory = Directory.CreateTempSubdirectory("library-edge-").FullName;
        try
        {
            using var named = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            use(named);
        }
        finally { Directory.Delete(directory, true); }
    }

    private static object? First(IEnumerable items)
    {
        var enumerator = items.GetEnumerator();
        return enumerator.MoveNext() ? enumerator.Current : null;
    }

    [Fact]
    public void LinkPartsReadTheSameUntyped()
    {
        var parts = new LinkParts<uint>(7, 8, 9);
        Assert.Equal(new object[] { 7u, 8u, 9u }, ((IEnumerable)parts).Cast<object>().ToArray());
    }

    [Fact]
    public void PinnedTypesReadTheSameUntyped()
    {
        using var store = new Store();
        var pinned = new PinnedTypes<uint>(store.Links);
        Assert.Equal(1u, First(pinned));
        Assert.Equal(1u, First(new PinnedTypesDecorator<uint>(store.Links)));
    }

    [Fact]
    public void AJournalRecordOfAnUnknownKindIsNotValid()
    {
        var id = Guid.Parse("00000000-0000-0000-0000-000000000001");
        var row = new Link<uint>(1u, 1u, 1u);
        var record = new JournalRecord<uint>(new Transition<uint>(id, 1, DateTimeOffset.UnixEpoch, (TransitionKind)99, row, row),
            names: new[] { new NameChange<uint>(1u, null, "named") });
        Assert.False(record.IsValid());
    }

    [Fact]
    public void AMapThatLeavesTheAddressesIsRefused()
    {
        WithNamed(named => Assert.Throws<OverflowException>(() => new RemapLinksDecorator<uint>(named, new ToNull()).ToOuter(1u)));
    }

    [Fact]
    public void TheNullLinkTakesNoExternalId()
    {
        WithNamed(named => Assert.Throws<ArgumentException>(() => new LinksExternalIds(named, "uuid").Bind("a", 0)));
    }

    [Fact]
    public void ACreatorThatRepeatsItselfIsRefused()
    {
        using var store = new Store();
        var message = Assert.Throws<InvalidOperationException>(() => store.Links.EnsureCreated(() => 5u, 9u)).Message;
        Assert.Contains("more than once", message);
    }

    [Fact]
    public void AVersionReplayWaitsForTheOpenTransaction()
    {
        var directory = Directory.CreateTempSubdirectory("replay-owner-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var tx = new TransactionsDecorator(data, log, trace: false);
            using var open = tx.BeginTransaction();
            Assert.Contains("still owns", Assert.Throws<InvalidOperationException>(tx.BeginVersionReplay).Message);
        }
        finally { Directory.Delete(directory, true); }
    }

    [Fact]
    public void ANameChangeNeedsASequenceToo()
    {
        var directory = Directory.CreateTempSubdirectory("name-exhaustion-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        try
        {
            uint a;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log, trace: false))
            {
                a = tx.CreateAndUpdate(0u, 0u);
                var id = Guid.NewGuid();
                var state = new Link<uint>(a, 0u, 0u);
                var filler = new Transition<uint>(id, long.MaxValue, DateTimeOffset.UnixEpoch, TransitionKind.Update, state, state);
                foreach (var entry in new[] { "__transactions:transition:" + filler.Serialize(),
                    "__transactions:commit:" + id.ToString("N"), "__transactions:applied:" + long.MaxValue })
                    log.SetName(log.CreateAndUpdate(0u, 0u), entry);
                data.Flush(); log.Flush();
            }
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log, trace: false))
            {
                Assert.Contains("exhausted", Assert.Throws<InvalidOperationException>(() => tx.SetName(a, "late")).Message);
                Assert.Null(tx.GetName(a));
            }
        }
        finally { Directory.Delete(directory, true); }
    }

    [Fact]
    public void ALineWrappedOnceMoreIsThatLine()
    {
        WithNamed(named =>
        {
            var triplets = new TripletsOverDoublets<uint>(named);
            var wrapped = TripletsLino.Import(triplets, "((Type: Type 0 0))\n((knows: Type 0 0))");
            Assert.Equal(2, wrapped.Count);
            Assert.Equal(wrapped[0], triplets.GetTriplet(wrapped[1])!.Value.Type);
        });
    }

    [Fact]
    public void BorrowedLinksKeepTheirNamesApart()
    {
        var directory = Directory.CreateTempSubdirectory("simple-borrowed-").FullName;
        try
        {
            using var store = new Store();
            using var simple = new SimpleLinksDecorator<uint>(store.Links, Path.Combine(directory, "names.links"));
            Assert.Equal(0u, simple.Count());
        }
        finally { Directory.Delete(directory, true); }
    }
}
