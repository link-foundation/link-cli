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
    description: "Path to the links database file"
);
dbOption.SetDefaultValue("db.links");

var queryOption = new Option<string>(
    name: "--query",
    description: "LiNo query for CRUD operation"
);

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store");
rootCommand.AddOption(dbOption);
rootCommand.AddOption(queryOption);

rootCommand.SetHandler((string db, string query) =>
{
    using var links = new UnitedMemoryLinks<uint>(db);

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
        Console.WriteLine("Processing query...");
        var parser = new Parser();
        var parsedLinks = parser.Parse(query);
        Console.WriteLine($"Parsed query: {parsedLinks.Format()}");

        ProcessLinks(links, parsedLinks);
    }
    catch (Exception ex)
    {
        Console.WriteLine($"Error: {ex.Message}");
    }
}, dbOption, queryOption);

await rootCommand.InvokeAsync(args);

void ProcessLinks(ILinks<uint> links, IList<LinoLink> parsedLinks)
{
    uint GetOrCreateAddress(string? id)
    {
        if (id == null)
        {
            Console.WriteLine("ID is null, returning default address.");
            return links.Constants.Null;
        }

        if (uint.TryParse(id, out uint address))
        {
            return address;
        }

        Console.WriteLine($"Invalid ID '{id}', returning default address.");
        return links.Constants.Null;
    }

    foreach (var link in parsedLinks)
    {
        try
        {
            if (link.Values == null || link.Values.Count < 2)
            {
                Console.WriteLine($"Link {link} does not have enough values.");
                continue;
            }

            var sourceId = link.Values[0].Id;
            var targetId = link.Values[1].Id;

            if (sourceId == null || targetId == null)
            {
                Console.WriteLine($"Link {link} has null Ids.");
                continue;
            }

            uint source = GetOrCreateAddress(sourceId);
            uint target = GetOrCreateAddress(targetId);

            if (link.Id == null)
            {
                Console.WriteLine($"Link {link} has null Id.");
                continue;
            }

            if (!uint.TryParse(link.Id, out uint linkId))
            {
                Console.WriteLine($"Invalid link Id '{link.Id}'.");
                continue;
            }

            var restriction = new List<uint> { linkId, links.Constants.Any, links.Constants.Any };
            var substitution = new List<uint> { linkId, source, target };

            Console.WriteLine($"Updating link: {string.Join(", ", substitution)}");
            links.Update(restriction, substitution, null);
        }
        catch (Exception ex)
        {
            Console.WriteLine($"Error processing link {link}: {ex.Message}");
        }
    }

    Console.WriteLine("Final data store contents:");
    links.Each(null, link =>
    {
        Console.WriteLine(links.Format(link));
        return links.Constants.Continue;
    });
}