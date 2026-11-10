using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineConstraintIndexTests
{
    private static EngineOptions Options(StateStructure structure, RevisionEncoding encoding, int mask) => new()
    {
        StateStructure = structure,
        RevisionEncoding = encoding,
        ConstraintValidation = ConstraintValidation.Indexed,
        Constraints = Enum.GetValues<EngineConstraint>().Where((_, bit) => (mask & (1 << bit)) != 0).ToArray(),
        Isolation = IsolationLevel.Snapshot,
        HistoryLimit = null
    };
    private static void Seed(LinksEngine<uint> engine)
    {
        using var seed = engine.Begin();
        foreach (var (id, name) in new[] { (1U, "a"), (2U, "雪\0"), (3U, "") })
        { seed.EnsureCreated(id); seed.UpdateLink(id, id, id); seed.SetName(id, name); }
        seed.Commit();
    }
    [Fact]
    public void IndexedConstraintsAllowAtomicSwapsAndPreserveRejectedCandidates()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new()
        {
            ConstraintValidation = ConstraintValidation.Indexed,
            Constraints = new[] { EngineConstraint.References, EngineConstraint.UniquePairs, EngineConstraint.UniqueNames }
        });
        using (var seed = engine.Begin())
        {
            for (uint id = 1; id <= 3; id++) { seed.EnsureCreated(id); seed.UpdateLink(id, id, id); seed.SetName(id, $"name {id}"); }
            seed.Commit();
        }
        using (var swap = engine.Begin())
        {
            swap.UpdateLink(1, 2, 2); swap.SetName(1, "name 2");
            swap.UpdateLink(2, 1, 1); swap.SetName(2, "name 1"); swap.Commit();
        }
        var before = engine.Snapshot();
        using (var bad = engine.Begin()) { bad.DeleteLink(1); Assert.Contains("reference", Assert.Throws<InvalidOperationException>(() => bad.Commit()).Message); }
        Assert.Same(before, engine.Snapshot());
        using var following = engine.Begin(); following.DeleteLink(1); following.DeleteLink(2); following.Commit();
        Assert.Single(engine.Snapshot().Rows);
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void FiniteIndependentOracleChecksAllConstraintSubsetsAndSingleRowEdits(StateStructure structure)
    {
        var original = new MemoryCommitLog(); Seed(new LinksEngine<uint>(original));
        foreach (var encoding in Enum.GetValues<RevisionEncoding>())
            for (int mask = 0; mask < 8; mask++)
                for (uint id = 1; id <= 2; id++)
                    for (uint source = 0; source <= 4; source++)
                        for (uint target = 0; target <= 4; target++)
                            foreach (var name in new string?[] { null, "a", "雪\0", "" })
                            {
                                var rows = new (uint Id, uint Source, uint Target, string? Name)[] { (1, 1, 1, "a"), (2, 2, 2, "雪\0"), (3, 3, 3, "") };
                                rows[id - 1] = (id, source, target, name);
                                bool references = rows.All(row => new[] { row.Source, row.Target }.All(reference => reference == 0 || rows.Any(candidate => candidate.Id == reference)));
                                bool pairs = rows.Select(row => (row.Source, row.Target)).Distinct().Count() == rows.Length;
                                var names = rows.Select(row => row.Name).OfType<string>().ToArray();
                                bool uniqueNames = names.Distinct(StringComparer.Ordinal).Count() == names.Length;
                                bool expected = ((mask & 1) == 0 || references) && ((mask & 2) == 0 || pairs) && ((mask & 4) == 0 || uniqueNames);
                                var log = new MemoryCommitLog(); foreach (var record in original.Read()) log.Append(record);
                                var engine = new LinksEngine<uint>(log, Options(structure, encoding, mask));
                                using var tx = engine.Begin(); tx.UpdateLink(id, source, target); if (name is { } text) tx.SetName(id, text); else tx.RemoveName(id);
                                var error = Record.Exception(() => tx.Commit()); Assert.Equal(expected, error is null);
                                if (!expected)
                                {
                                    Assert.IsType<InvalidOperationException>(error); Assert.Equal(1UL, engine.Snapshot().Revision); Assert.Single(log.Read());
                                    using var following = engine.Begin(); following.SetName(3, "following"); following.Commit();
                                }
                            }
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void BothReferencePositionsAndFinalCandidateDetermineValidity(StateStructure structure)
    {
        foreach (var encoding in Enum.GetValues<RevisionEncoding>())
        {
            var log = new MemoryCommitLog(); var options = Options(structure, encoding, 1); var engine = new LinksEngine<uint>(log, options); Seed(engine);
            using (var refs = engine.Begin()) { refs.UpdateLink(2, 1, 1); refs.UpdateLink(3, 1, 0); refs.Commit(); }
            using (var one = engine.Begin()) { one.UpdateLink(2, 2, 0); one.Commit(); }
            using (var bad = engine.Begin()) { bad.DeleteLink(1); Assert.Throws<InvalidOperationException>(() => bad.Commit()); }
            using (var remove = engine.Begin()) { remove.DeleteLink(1); remove.DeleteLink(3); remove.Commit(); }
            using (var transient = engine.Begin()) { uint id = transient.CreateLink(99, 99); transient.DeleteLink(id); transient.Commit(); }
            var reopened = new LinksEngine<uint>(log, options);
            using (var bad = reopened.Begin()) { bad.UpdateLink(2, 1, 0); Assert.Throws<InvalidOperationException>(() => bad.Commit()); }
            using var good = reopened.Begin(); good.EnsureCreated(1); good.UpdateLink(1, 2, 0); good.UpdateLink(2, 1, 0); good.Commit();
        }
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void GroupsBranchesRecoveryAndPinsKeepTheirOwnIndexRoots(StateStructure structure)
    {
        foreach (var encoding in Enum.GetValues<RevisionEncoding>())
        {
            var log = new MemoryCommitLog(); var options = Options(structure, encoding, 7); var engine = new LinksEngine<uint>(log, options); Seed(engine); var pinned = engine.Snapshot();
            using var a = engine.Begin(); using var b = engine.Begin(); a.SetName(1, "group"); b.SetName(2, "group");
            var outcomes = engine.CommitGroup(new[] { a, b }); Assert.Equal(GroupCommitStatus.Committed, outcomes[0].Status); Assert.Equal(2UL, outcomes[0].Revision); Assert.Equal(GroupCommitStatus.Aborted, outcomes[1].Status);
            Assert.Equal("a", pinned.Rows[1].Name);
            foreach (var validation in Enum.GetValues<ConstraintValidation>())
            {
                var branch = engine.Fork(new MemoryCommitLog(), options with { ConstraintValidation = validation, Constraints = Array.Empty<EngineConstraint>() });
                using (var edit = branch.Engine.Begin()) { edit.SetName(2, "group"); edit.Commit(); }
                var before = engine.Snapshot(); Assert.Throws<InvalidOperationException>(() => branch.Merge()); Assert.Same(before, engine.Snapshot());
                using (var fix = branch.Engine.Begin()) { fix.SetName(2, $"child {validation}"); fix.Commit(); }
                ulong revision = branch.Merge(); Assert.Equal(revision, branch.Merge());
            }
            var saved = log.Read();
            foreach (var validation in Enum.GetValues<ConstraintValidation>())
            {
                var detached = new MemoryCommitLog(); foreach (var record in saved) detached.Append(record);
                var reopened = new LinksEngine<uint>(detached, options with { ConstraintValidation = validation });
                using (var bad = reopened.Begin()) { bad.SetName(3, "group"); Assert.Throws<InvalidOperationException>(() => bad.Commit()); }
                using var transfer = reopened.Begin(); transfer.RemoveName(1); transfer.SetName(3, "group"); transfer.Commit();
            }
        }
    }

    [Fact]
    public void RecoveryValidatesHistoricalConstraintsBeforeBuildingFinalIndexes()
    {
        var log = new MemoryCommitLog(); var engine = new LinksEngine<uint>(log); Seed(engine);
        using (var bad = engine.Begin()) { bad.SetName(2, "a"); bad.Commit(); }
        using (var fix = engine.Begin()) { fix.SetName(2, "fixed"); fix.Commit(); }
        foreach (var structure in Enum.GetValues<StateStructure>()) Assert.Throws<InvalidOperationException>(() => new LinksEngine<uint>(log, Options(structure, RevisionEncoding.Changes, 4)));
        Assert.Throws<ArgumentException>(() => new LinksEngine<uint>(new MemoryCommitLog(), new() { ConstraintValidation = (ConstraintValidation)99 }));
    }
}
