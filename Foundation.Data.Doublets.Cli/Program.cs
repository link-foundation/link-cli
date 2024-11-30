

using System;
using System.CommandLine;
using System.CommandLine.Invocation;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Data.Doublets;
using Platform.Protocols.Lino;

Console.WriteLine("Welcome to LiNo CLI Tool!");

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
    new Option<string>(
        name: "--db",
        description: "Path to the links database file",
        getDefaultValue: () => "db.links"),
    new Option<string>(
        name: "--query",
        description: "LiNo query for CRUD operation")
};

rootCommand.Handler = CommandHandler.Create<string, string>((db, query) =>
{
    using var links = new UnitedMemoryLinks<uint>(db);
    var parser = new Parser();

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

        ProcessLinks(links, parsedLinks);
    }
    catch (Exception ex)
    {
        Console.WriteLine($"Error processing query: {ex.Message}");
    }
});

return await rootCommand.InvokeAsync(args);

static void ProcessLinks(ILinks<uint> links, Links<uint> parsedLinks)
{
    foreach (var link in parsedLinks)
    {
        if (link.Source == 0 && link.Target == 0)
        {
            // Create new link
            var newLink = links.Create();
            links.Update(newLink, link.Index, link.Source, link.Target);
            Console.WriteLine($"Created link: {links.Format(newLink)}");
        }
        else if (link.Target == 0)
        {
            // Delete link
            links.Delete(link.Index);
            Console.WriteLine($"Deleted link with index: {link.Index}");
        }
        else
        {
            // Update link
            links.Update(link.Index, link.Source, link.Target);
            Console.WriteLine($"Updated link: {links.Format(link)}");
        }
    }

    Console.WriteLine("Final data store contents:");
    links.Each(link =>
    {
        Console.WriteLine(links.Format(link));
        return links.Constants.Continue;
    });
}