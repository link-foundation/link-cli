using Platform.Data.Doublets;
using Foundation.Data.Doublets.Cli;
using static Foundation.Data.Doublets.Cli.ChangesSimplifier;

// Test the problematic scenario from the issue
var changes = new List<(Link<uint> Before, Link<uint> After)>
{
    // This simulates what might be happening in the update operation
    // Let's say we have links being swapped: (1:1 2) -> (1:2 1) and (2:2 1) -> (2:1 2)
    
    // From the issue it looks like these might be the intermediate steps:
    (new Link<uint>(index: 1, source: 1, target: 2), new Link<uint>(index: 1, source: 2, target: 1)),
    (new Link<uint>(index: 2, source: 2, target: 1), new Link<uint>(index: 2, source: 1, target: 2)),
};

Console.WriteLine("Original changes:");
foreach (var (before, after) in changes)
{
    Console.WriteLine($"({before.Index}: {before.Source} {before.Target}) -> ({after.Index}: {after.Source} {after.Target})");
}

Console.WriteLine("\nSimplified changes:");
var simplified = SimplifyChanges(changes).ToList();
foreach (var (before, after) in simplified)
{
    Console.WriteLine($"({before.Index}: {before.Source} {before.Target}) -> ({after.Index}: {after.Source} {after.Target})");
}