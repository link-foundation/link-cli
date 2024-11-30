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

    Console.WriteLine("Processing query...");
    var parser = new Parser();
    var parsedLinks = parser.Parse(query);
    Console.WriteLine($"Parsed query: {parsedLinks.Format()}");

    // Process parsed links based on CRUD operations
    ProcessLinks(links, parsedLinks);

}, dbOption, queryOption);

await rootCommand.InvokeAsync(args);

void ProcessLinks(ILinks<uint> links, IList<LinoLink> parsedLinks)
{
    uint GetLinkAddress(LinoLink link)
    {
        if (link.Id != null && uint.TryParse(link.Id, out uint linkId))
        {
            return linkId;
        }
        else
        {
            Console.WriteLine("Link does not have a valid Id.");
            return links.Constants.Null;
        }
    }

    if (parsedLinks.Count == 0)
    {
        Console.WriteLine("No links in the parsed query.");
        return;
    }

    // Get the outer link
    var outerLink = parsedLinks[0];

    if (outerLink.Values == null || outerLink.Values.Count < 2)
    {
        Console.WriteLine("Not enough links in the query for an update operation.");
        return;
    }

    // Assume first value is restriction, second is substitution
    var restrictionLink = outerLink.Values[0];
    var substitutionLink = outerLink.Values[1];

    uint linkId = GetLinkAddress(restrictionLink);

    uint restrictionSource = links.Constants.Any;
    uint restrictionTarget = links.Constants.Any;

    if (restrictionLink.Values != null && restrictionLink.Values.Count == 2)
    {
        restrictionSource = GetLinkAddress(restrictionLink.Values[0]);
        restrictionTarget = GetLinkAddress(restrictionLink.Values[1]);
    }

    uint substitutionSource = links.Constants.Any;
    uint substitutionTarget = links.Constants.Any;

    if (substitutionLink.Values != null && substitutionLink.Values.Count == 2)
    {
        substitutionSource = GetLinkAddress(substitutionLink.Values[0]);
        substitutionTarget = GetLinkAddress(substitutionLink.Values[1]);
    }

    var restriction = new List<uint> { linkId, restrictionSource, restrictionTarget };
    var substitution = new List<uint> { linkId, substitutionSource, substitutionTarget };

    Console.WriteLine($"Updating link with restriction: {string.Join(", ", restriction)}");
    Console.WriteLine($"Updating link with substitution: {string.Join(", ", substitution)}");

    links.Update(restriction, substitution, null);

    Console.WriteLine("Final data store contents:");
    links.Each(null, link =>
    {
        Console.WriteLine(links.Format(link));
        return links.Constants.Continue;
    });
}