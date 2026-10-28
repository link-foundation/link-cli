// Finite missing-API contract. Existing row setup avoids probing allocations.
using Foundation.Data.Doublets.Cli;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

var engine = new LinksEngine<UInt128>(new MemoryCommitLog());
using var tx = engine.Begin();
UInt128 high = ulong.MaxValue;
tx.EnsureCreated(1);
tx.UpdateLink(1, 1, 1);
tx.EnsureCreated(high);
tx.UpdateLink(high, high, high);
tx.SetName(1, high.ToString());
int observed = 0;
AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(tx, new()
{
    Query = $"(({high}: {high} {high})) (({high}: {high} {high}))",
    ChangesHandler = (before, after) =>
    {
        ArgumentNullException.ThrowIfNull(before);
        ArgumentNullException.ThrowIfNull(after);
        if (before[0] != high || after[0] != high)
            throw new Exception("The full u64 identity was interpreted as a name or wildcard.");
        observed++;
        return tx.Constants.Continue;
    },
});
AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(tx,
    $"(('{high}': '{high}' '{high}')) (('{high}': '{high}' '{high}'))");
if (observed != 1 || !Equals(tx.GetLink(high), new Link<UInt128>(high, high, high))
    || !Equals(tx.GetLink(1), new Link<UInt128>(1, 1, 1)) || engine.Snapshot().Rows.Count != 0)
    throw new Exception("Generic queries lost an address/name kind, changed a point or published staging.");
Console.WriteLine("PASS: shared generic query API preserves full u64 addresses and quoted numeric names in private UInt128 rows.");
