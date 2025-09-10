using System;
using System.Collections.Generic;
using System.Linq;
using Platform.Data.Doublets;
using Foundation.Data.Doublets.Cli;
using static Foundation.Data.Doublets.Cli.ChangesSimplifier;

// Simulate the exact scenario from the issue
Console.WriteLine("=== Testing Issue #26 Scenario ===");

// This simulates what might be happening based on the issue output:
// ((1: 1 2)) ()           - Link (1: 1 2) is deleted  
// ((1: 1 2)) ((1: 2 1))   - Link (1: 1 2) becomes (1: 2 1)
// ((2: 2 1)) ((2: 1 2))   - Link (2: 2 1) becomes (2: 1 2)

var changes = new List<(Link<uint> Before, Link<uint> After)>
{
    // First transformation: (1: 1 2) -> () (deletion)
    (new Link<uint>(index: 1, source: 1, target: 2), new Link<uint>(index: 0, source: 0, target: 0)),
    
    // Second transformation: () -> (1: 2 1) (creation)
    (new Link<uint>(index: 0, source: 0, target: 0), new Link<uint>(index: 1, source: 2, target: 1)),
    
    // Third transformation: (2: 2 1) -> (2: 1 2) (direct update)
    (new Link<uint>(index: 2, source: 2, target: 1), new Link<uint>(index: 2, source: 1, target: 2)),
};

Console.WriteLine("Original changes (as they might occur in the system):");
for (int i = 0; i < changes.Count; i++)
{
    var (before, after) = changes[i];
    Console.WriteLine($"{i + 1}. ({before.Index}: {before.Source} {before.Target}) -> ({after.Index}: {after.Source} {after.Target})");
}

Console.WriteLine("\nSimplified changes (what should be shown):");
var simplified = SimplifyChanges(changes).ToList();
for (int i = 0; i < simplified.Count; i++)
{
    var (before, after) = simplified[i];
    Console.WriteLine($"{i + 1}. ({before.Index}: {before.Source} {before.Target}) -> ({after.Index}: {after.Source} {after.Target})");
}

Console.WriteLine("\n=== Expected vs Actual ===");
Console.WriteLine("Expected: Only the final transformations should be shown");
Console.WriteLine("- (1: 1 2) -> (1: 2 1)");
Console.WriteLine("- (2: 2 1) -> (2: 1 2)");

Console.WriteLine($"\nActual count: {simplified.Count}");
Console.WriteLine("If this count is more than 2, then the issue is reproduced.");