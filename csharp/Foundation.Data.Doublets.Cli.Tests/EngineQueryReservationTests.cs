using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed class EngineQueryReservationTests
{
    [Fact]
    public void PreviewFailureLeavesRowsNamesAndAllocatorUntouched()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        Assert.Throws<FormatException>(() => tx.ReserveQueryAllocations(preview =>
        {
            preview.Create(null, null);
            preview.SetName(1, "private");
            preview.TryEnsureCreated(100);
            throw new FormatException("Rejected preview.");
        }));
        Assert.Equal(0u, tx.Count(null));
        Assert.Equal(0u, tx.GetByName("private"));
        Assert.Equal(1u, tx.CreateLink(0, 0));
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void PreviewRowsNamesCallbacksAndReturnedIdentitiesAreDetached()
    {
        var engine = new LinksEngine<UInt128>(new MemoryCommitLog());
        using var tx = engine.Begin();
        INamedTypesLinks<UInt128>? retained = null;
        IList<UInt128>? observed = null;
        var ids = tx.ReserveQueryAllocations(preview =>
        {
            retained = preview;
            preview.Create(null, (_, row) => { observed = row; return preview.Constants.Continue; });
            preview.SetName(1, "preview");
            preview.Update([1], [1, ulong.MaxValue, 0], null);
        });
        Assert.Equal(new UInt128[] { 1 }, ids!);
        Assert.Equal(new UInt128[] { 1, 0, 0 }, observed!);
        Assert.Throws<NotSupportedException>(() => observed![0] = 2);
        retained!.SetName(1, "following preview edit");
        retained.Delete([1], null);
        Assert.Equal(UInt128.One, tx.CreateLink(0, 0));
        Assert.Null(tx.GetName(1));
        Assert.Equal(new UInt128[] { 1, 0, 0 }, observed!);
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void EightSnapshotOwnersReserveDisjointIdentitiesBeforeAnyCommit()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new() { Isolation = IsolationLevel.Snapshot });
        var owners = Enumerable.Range(0, 8).Select(_ => engine.Begin()).ToArray();
        try
        {
            foreach (var owner in owners) owner.ReserveQueryAllocations(preview => preview.Create(null, null));
            for (var i = owners.Length - 1; i >= 0; i--)
            {
                Assert.Equal((uint)i + 1, owners[i].CreateLink(0, 0));
                owners[i].Commit();
            }
            Assert.Equal(Enumerable.Range(1, 8).Select(i => (uint)i), engine.Snapshot().Rows.Keys);
        }
        finally { foreach (var owner in owners) owner.Dispose(); }
    }

    [Fact]
    public void BusyWriteDoesNotConsumeTheOwnersReservation()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new()
        { Isolation = IsolationLevel.Snapshot, Concurrency = ConcurrencyControl.Pessimistic });
        using var owner = engine.Begin();
        owner.ReserveQueryAllocations(preview => preview.Create(null, null));
        using (var blocker = engine.Begin())
        {
            blocker.EnsureCreated(1);
            Assert.Contains("Busy", Assert.Throws<InvalidOperationException>(() => owner.CreateLink(0, 0)).Message);
        }
        Assert.Equal(1u, owner.CreateLink(0, 0));
        owner.Commit();
        Assert.Single(engine.Snapshot().Rows);
    }

    [Fact]
    public void AVisibleExplicitCollisionDoesNotConsumeAReservedIdentity()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new() { Isolation = IsolationLevel.ReadCommitted });
        using var owner = engine.Begin();
        owner.ReserveQueryAllocations(preview => preview.Create(null, null));
        using (var writer = engine.Begin()) { writer.EnsureCreated(1); writer.Commit(); }
        Assert.Throws<InvalidOperationException>(() => owner.ReserveQueryAllocations(preview => preview.Create(null, null)));
        Assert.Contains("already exists", Assert.Throws<InvalidOperationException>(() => owner.CreateLink(0, 0)).Message);
        using (var writer = engine.Begin()) { writer.DeleteLink(1); writer.Commit(); }
        Assert.Equal(1u, owner.CreateLink(0, 0));
        Assert.Equal(2u, owner.CreateLink(0, 0));
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void PreviewScanParticipatesInSerializableValidation()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new()
        { SerializableValidation = SerializableValidation.ReadSet });
        using var reader = engine.Begin();
        reader.ReserveQueryAllocations(preview => Assert.Equal(0u, preview.Count(null)));
        using (var writer = engine.Begin()) { writer.CreateLink(0, 0); writer.Commit(); }
        Assert.Throws<InvalidOperationException>(() => reader.Commit());
    }

    [Fact]
    public void RollbackRetiresReservationsAndClosedOwnersRejectPreview()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var first = engine.Begin();
        Assert.Throws<ArgumentNullException>(() => first.ReserveQueryAllocations(null!));
        first.ReserveQueryAllocations(preview => preview.Create(null, null));
        first.Rollback();
        Assert.Throws<InvalidOperationException>(() => first.ReserveQueryAllocations(_ => { }));
        using var following = engine.Begin();
        Assert.Equal(2u, following.CreateLink(0, 0));
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void DirectWideEnsureDoesNotWalkInterveningAddressesAndNamesRemainOwned()
    {
        var engine = new LinksEngine<UInt128>(new MemoryCommitLog());
        using var tx = engine.Begin();
        INamedTypesLinks<UInt128> bounded = new BoundedWideStore(tx);
        UInt128 high = ulong.MaxValue;
        LinksExtensions.EnsureCreated(bounded, high);
        bounded.SetName(high, "wide name");
        var names = bounded.CaptureNames();
        bounded.SetName(high, "following name");
        Assert.Equal("wide name", names[high]);
        Assert.Equal(new Link<UInt128>(high, 0, 0), tx.GetLink(high));
        Assert.Equal(high + 1, tx.CreateLink(0, 0));
        Assert.Empty(engine.Snapshot().Rows);
    }

    [Fact]
    public void EnsureValidatesEveryAddressBeforeWritingAndPreservesCustomCreators()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        Assert.Throws<InvalidOperationException>(() => LinksExtensions.EnsureCreated(tx, 1u, 0u));
        Assert.Equal(0u, tx.Count(null));
        Assert.Equal(1u, tx.CreateLink(0, 0));
        var calls = 0;
        LinksExtensions.EnsureCreated(tx, () => { calls++; return tx.CreateLink(0, 0); }, 3u);
        Assert.Equal(2, calls);
        Assert.Null(tx.GetLink(2));
        Assert.NotNull(tx.GetLink(3));
    }

    // The allocation cap makes a regression to iterative ensure fail after four
    // rows, rather than trying to walk the full u64 address space.
    private sealed class BoundedWideStore(EngineSession<UInt128> owner) : INamedTypesLinks<UInt128>
    {
        private int _creates;
        public LinksConstants<UInt128> Constants => owner.Constants;
        public bool TryEnsureCreated(UInt128 id) => owner.TryEnsureCreated(id);
        public UInt128 Create(IList<UInt128>? substitution, WriteHandler<UInt128>? handler)
        {
            if (++_creates > 4) throw new InvalidOperationException("Finite allocation guard exceeded.");
            return owner.Create(substitution, handler);
        }
        public UInt128 Update(IList<UInt128>? restriction, IList<UInt128>? substitution, WriteHandler<UInt128>? handler)
            => owner.Update(restriction, substitution, handler);
        public UInt128 Delete(IList<UInt128>? restriction, WriteHandler<UInt128>? handler) => owner.Delete(restriction, handler);
        public UInt128 Count(IList<UInt128>? restriction) => owner.Count(restriction);
        public UInt128 Each(IList<UInt128>? restriction, ReadHandler<UInt128>? handler) => owner.Each(restriction, handler);
        public string? GetName(UInt128 id) => owner.GetName(id);
        public UInt128 GetByName(string name) => owner.GetByName(name);
        public UInt128 SetName(UInt128 id, string name) => owner.SetName(id, name);
        public void RemoveName(UInt128 id) => owner.RemoveName(id);
    }
}
