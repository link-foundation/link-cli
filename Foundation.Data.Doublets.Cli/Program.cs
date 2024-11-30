using System;
using System.CommandLine;
using System.Collections.Generic;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Data.Doublets;
using Platform.Protocols.Lino;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using Platform.Data;

Console.WriteLine("Welcome to LiNo CLI Tool!");

// Define options
var dbOption = new Option<string>(
    name: "--db",
    description: "Path to the links database file",
    getDefaultValue: () => "db.links");

var queryOption = new Option<string>(
    name: "--query",
    description: "LiNo query for CRUD operation");

// Create root command
var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store");
rootCommand.AddOption(dbOption);
rootCommand.AddOption(queryOption);

// Set handler using the options directly
rootCommand.SetHandler((string db, string query) =>
{
    using var links = new UnitedMemoryLinks<uint>(db);
    var parser = new Parser();

    var link1 = links.GetOrCreate(1u, 1u);
    var link2 = links.GetOrCreate(2u, 2u);
    Console.WriteLine($"Created link: {links.Format(link1)}");
    Console.WriteLine($"Created link: {links.Format(link2)}");

    if (string.IsNullOrWhiteSpace(query))
    {
        Console.WriteLine("No query provided.");
        return;
    }

    try
    {
        var parsedLinks = parser.Parse(query);
        Console.WriteLine("Parsed query successfully:");
        Console.WriteLine(parsedLinks.Format());

        // Process parsed links based on CRUD operations
        ProcessLinks(links, parsedLinks);
    }
    catch (Exception ex)
    {
        Console.WriteLine($"Error processing query: {ex.Message}");
    }
}, dbOption, queryOption);

await rootCommand.InvokeAsync(args);

void ProcessLinks(ILinks<uint> links, IList<LinoLink> parsedLinks)
{
    // Mapping between string identifiers and link addresses
    var identifiers = new Dictionary<string, uint>();

    // Process each parsed link
    foreach (var link in parsedLinks)
    {
        ProcessLinoLink(link, null);
    }

    // Helper method to process a LinoLink
    void ProcessLinoLink(LinoLink linoLink, uint? parentAddress)
    {
        uint currentAddress = 0;

        // If the link has an Id, get or create the address for it
        if (linoLink.Id != null)
        {
            currentAddress = GetOrCreateAddress(linoLink.Id);
        }
        else if (linoLink.Values != null && linoLink.Values.Count > 0)
        {
            // If the link doesn't have an Id but has values, create a new link
            currentAddress = links.Create(null, null);
            Console.WriteLine($"Created link with address: {currentAddress}");
        }

        // If there is a parent, connect it
        if (parentAddress.HasValue)
        {
            var restriction = new List<uint> { parentAddress.Value, links.Constants.Any, links.Constants.Any };
            var substitution = new List<uint> { parentAddress.Value, parentAddress.Value, currentAddress };
            links.Update(restriction, substitution, null);
            Console.WriteLine($"Connected parent {parentAddress.Value} to {currentAddress}");
        }

        // Process values recursively
        if (linoLink.Values != null)
        {
            foreach (var childLink in linoLink.Values)
            {
                ProcessLinoLink(childLink, currentAddress);
            }
        }
    }

    uint GetOrCreateAddress(string id)
    {
        if (uint.TryParse(id, out uint address))
        {
            // If id is already a uint number
            return address;
        }

        if (!identifiers.TryGetValue(id, out address))
        {
            // Create a new link to represent this identifier
            address = links.Create(null, null);
            identifiers[id] = address;
            Console.WriteLine($"Created identifier '{id}' with address {address}");
        }

        return address;
    }

    // After processing, print the final data store contents
    Console.WriteLine("Final data store contents:");
    links.Each(null, link =>
    {
        Console.WriteLine(links.Format(link));
        return links.Constants.Continue;
    });
}