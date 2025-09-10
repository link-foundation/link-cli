using System;
using System.Collections.Generic;
using System.Linq;

class MockLinks
{
    private uint counter = 1;
    private Dictionary<uint, bool> existingLinks = new Dictionary<uint, bool>();
    
    public uint Create()
    {
        // This simulates the infinite loop by always returning the same value
        // that never matches the 'max' target
        return counter; // Always return 1, never reaching higher values
    }
    
    public bool Exists(uint id) => existingLinks.ContainsKey(id);
}

class Program
{
    static void Main()
    {
        Console.WriteLine("Testing LinksExtensions fix for infinite loop...");
        
        var mockLinks = new MockLinks();
        var addresses = new uint[] { 5, 10, 15 }; // Try to create these addresses
        
        try
        {
            // This would previously cause an infinite loop because mockLinks.Create() 
            // always returns 1, never reaching the max target of 15
            TestEnsureCreated(mockLinks, addresses);
            Console.WriteLine("Test failed - expected InvalidOperationException");
        }
        catch (InvalidOperationException ex)
        {
            Console.WriteLine($"SUCCESS: Caught expected exception: {ex.Message}");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"FAILURE: Unexpected exception type: {ex.GetType()}: {ex.Message}");
        }
    }
    
    static void TestEnsureCreated(MockLinks mockLinks, uint[] addresses)
    {
        // Simplified version of the LinksExtensions.EnsureCreated logic
        var nonExistentAddresses = new HashSet<uint>();
        foreach (var addr in addresses)
        {
            if (!mockLinks.Exists(addr))
            {
                nonExistentAddresses.Add(addr);
            }
        }
        
        if (nonExistentAddresses.Count > 0)
        {
            var max = nonExistentAddresses.Max();
            var createdLinks = new List<uint>();
            var seenAddresses = new HashSet<uint>();
            uint createdLink;
            var maxIterations = 10000;
            var iterations = 0;
            
            do
            {
                createdLink = mockLinks.Create();
                
                // Check for infinite loop conditions first
                if (iterations++ > maxIterations)
                {
                    throw new InvalidOperationException($"Link creation exceeded maximum iterations ({maxIterations}). This may indicate a circular reference or infinite recursion in the link creation process.");
                }
                
                // Early break if we're in an obvious cycle
                if (createdLinks.Count > 0 && seenAddresses.Contains(createdLink) && createdLink != max)
                {
                    // If we've created many links and started seeing repeats (but not the target), likely infinite loop
                    if (createdLinks.Count > 50)
                    {
                        throw new InvalidOperationException($"Link creation appears to be in an infinite loop. Created {createdLinks.Count} links, seeing repeated address {createdLink}, but target {max} not reached.");
                    }
                }
                
                seenAddresses.Add(createdLink);
                createdLinks.Add(createdLink);
                
                // Additional safety: if we've created far more links than the target ID suggests, something is wrong
                if (createdLinks.Count > Math.Max(100, (int)(max * 2)))
                {
                    throw new InvalidOperationException($"Link creation created {createdLinks.Count} links while trying to reach {max}. This suggests infinite recursion.");
                }
            }
            while (createdLink != max);
        }
    }
}