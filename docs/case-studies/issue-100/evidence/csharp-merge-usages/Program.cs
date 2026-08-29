// Isolates `ILinksExtensions.MergeUsages` in Platform.Data.Doublets 0.18.1.
//
// The store below carries no decorators, so nothing but `MergeUsages` can
// touch the links: whatever the dump prints is what `MergeUsages` wrote.
//
// `MergeUsages(old, new)` is supposed to repoint every reference to `old` at
// `new` and leave the other half of each doublet alone. It builds its
// substitutions with the two-argument `Link<TLinkAddress>` constructor:
//
//     var substitution = new Link<TLinkAddress>(newLinkIndex, links.GetTarget(usageAsSource));
//     var substitution = new Link<TLinkAddress>(links.GetTarget(usageAsTarget), newLinkIndex);
//
// but that constructor takes `params TLinkAddress[] values`, and `SetValues`
// reads a two-element list as `(index, source)` with a *null target* -- not as
// `(source, target)`. Both substitutions therefore land in the wrong slots.
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;

var databaseFilename = Path.Combine(Path.GetTempPath(), $"merge-usages-{Guid.NewGuid():N}.links");
var reproduced = 0;
try
{
    using var links = new UnitedMemoryLinks<uint>(databaseFilename);

    // `one` is merged away, `two` survives, `three` is an unrelated address
    // that both usages keep on their other half.
    var one = links.CreateAndUpdate(links.Constants.Null, links.Constants.Null);
    var two = links.CreateAndUpdate(links.Constants.Null, links.Constants.Null);
    var three = links.CreateAndUpdate(links.Constants.Null, links.Constants.Null);

    var usageAsSource = links.CreateAndUpdate(one, three);
    var usageAsTarget = links.CreateAndUpdate(three, one);

    Console.WriteLine($"before: {Dump(links)}");
    links.MergeUsages(one, two);
    Console.WriteLine($"after:  {Dump(links)}");

    // Repointing a usage must replace only the half that named `one`.
    Check("usage as source", usageAsSource, two, three);
    Check("usage as target", usageAsTarget, three, two);

    void Check(string what, uint index, uint expectedSource, uint expectedTarget)
    {
        var link = links.GetLink(index);
        var source = links.GetSource(link);
        var target = links.GetTarget(link);
        if (source == expectedSource && target == expectedTarget)
        {
            Console.WriteLine($"FIXED {what}: ({index}: {source} {target}) is what a correct merge produces");
            return;
        }
        reproduced++;
        Console.WriteLine($"BUG   {what}: expected ({index}: {expectedSource} {expectedTarget}), got ({index}: {source} {target})");
    }
}
finally
{
    File.Delete(databaseFilename);
}
// Exit 0 while the defect reproduces -- this harness records upstream
// behaviour rather than asserting it. It turns red once both usages survive
// the merge intact, which is the signal to drop the parity exemption in
// ../cli-parity/run.sh and re-check the pinned Platform.Data.Doublets version.
if (reproduced == 2)
{
    Console.WriteLine("MergeUsages still corrupts both kinds of usage.");
    return 0;
}
Console.WriteLine("MergeUsages no longer matches the recorded behaviour -- revisit the case study.");
return 1;

static string Dump(ILinks<uint> links)
{
    var parts = new List<string>();
    links.Each(new Link<uint>(links.Constants.Any, links.Constants.Any, links.Constants.Any), link =>
    {
        parts.Add($"({links.GetIndex(link)}: {links.GetSource(link)} {links.GetTarget(link)})");
        return links.Constants.Continue;
    });
    return string.Join(" ", parts);
}
