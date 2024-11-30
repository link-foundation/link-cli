using System;
using System.CommandLine;
using System.Collections.Generic;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using Platform.Data;

namespace LiNoCliTool
{
    class Program
    {
        static async System.Threading.Tasks.Task Main(string[] args)
        {
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

                // Create initial test data
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
        }

        static void ProcessLinks(ILinks<uint> links, IList<LinoLink> parsedLinks)
        {
            uint GetLinkAddress(LinoLink link)
            {
                if (link == null)
                {
                    Console.WriteLine("Link is null.");
                    return links.Constants.Null;
                }

                if (!string.IsNullOrEmpty(link.Id) && uint.TryParse(link.Id, out uint linkId))
                {
                    Console.WriteLine($"Found link Id: {linkId}");
                    return linkId;
                }
                else if (link.Values != null && link.Values.Count > 0)
                {
                    foreach (var value in link.Values)
                    {
                        uint id = GetLinkAddress(value);
                        if (id != links.Constants.Null)
                        {
                            return id;
                        }
                    }
                }
                Console.WriteLine("Link does not have a valid Id.");
                return links.Constants.Null;
            }

            void PrintLinoLink(LinoLink link, int indent = 0)
            {
                var indentStr = new string(' ', indent * 2);
                Console.WriteLine($"{indentStr}Link Id: {link.Id}");
                if (link.Values != null && link.Values.Count > 0)
                {
                    Console.WriteLine($"{indentStr}Values:");
                    foreach (var value in link.Values)
                    {
                        PrintLinoLink(value, indent + 1);
                    }
                }
            }

            // Print the parsed links
            Console.WriteLine("Detailed Parsed Links Structure:");
            foreach (var link in parsedLinks)
            {
                PrintLinoLink(link);
            }

            if (parsedLinks.Count == 0)
            {
                Console.WriteLine("No links in the parsed query.");
                return;
            }

            var outerLink = parsedLinks[0];

            if (outerLink.Values == null || outerLink.Values.Count < 2)
            {
                Console.WriteLine("Not enough links in the query for an update operation.");
                return;
            }

            var restrictionLink = outerLink.Values[0];
            var substitutionLink = outerLink.Values[1];

            uint linkId = links.Constants.Null;
            uint restrictionSource = links.Constants.Null;
            uint restrictionTarget = links.Constants.Null;
            uint substitutionSource = links.Constants.Null;
            uint substitutionTarget = links.Constants.Null;

            // Process restrictionLink
            if (restrictionLink.Values != null && restrictionLink.Values.Count > 0)
            {
                var restrictionInnerLink = restrictionLink.Values[0];
                linkId = GetLinkAddress(restrictionInnerLink);

                if (linkId == links.Constants.Null)
                {
                    Console.WriteLine("Failed to retrieve linkId from restriction.");
                    return;
                }

                if (restrictionInnerLink.Values != null && restrictionInnerLink.Values.Count >= 2)
                {
                    restrictionSource = GetLinkAddress(restrictionInnerLink.Values[0]);
                    restrictionTarget = GetLinkAddress(restrictionInnerLink.Values[1]);

                    if (restrictionSource == links.Constants.Null || restrictionTarget == links.Constants.Null)
                    {
                        Console.WriteLine("Failed to retrieve restriction source or target.");
                        return;
                    }
                }
                else
                {
                    Console.WriteLine("Restriction inner link does not have enough values.");
                    return;
                }
            }
            else
            {
                Console.WriteLine("Restriction link does not have values.");
                return;
            }

            // Process substitutionLink
            if (substitutionLink.Values != null && substitutionLink.Values.Count > 0)
            {
                var substitutionInnerLink = substitutionLink.Values[0];

                if (substitutionInnerLink.Values != null && substitutionInnerLink.Values.Count >= 2)
                {
                    substitutionSource = GetLinkAddress(substitutionInnerLink.Values[0]);
                    substitutionTarget = GetLinkAddress(substitutionInnerLink.Values[1]);

                    if (substitutionSource == links.Constants.Null || substitutionTarget == links.Constants.Null)
                    {
                        Console.WriteLine("Failed to retrieve substitution source or target.");
                        return;
                    }
                }
                else
                {
                    Console.WriteLine("Substitution inner link does not have enough values.");
                    return;
                }
            }
            else
            {
                Console.WriteLine("Substitution link does not have values.");
                return;
            }

            // Print out the IDs before update
            Console.WriteLine($"linkId: {linkId}");
            Console.WriteLine($"restrictionSource: {restrictionSource}");
            Console.WriteLine($"restrictionTarget: {restrictionTarget}");
            Console.WriteLine($"substitutionSource: {substitutionSource}");
            Console.WriteLine($"substitutionTarget: {substitutionTarget}");

            var restriction = new List<uint> { linkId, restrictionSource, restrictionTarget };
            var substitution = new List<uint> { linkId, substitutionSource, substitutionTarget };

            Console.WriteLine($"Updating link with restriction: {string.Join(", ", restriction)}");
            Console.WriteLine($"Updating link with substitution: {string.Join(", ", substitution)}");

            links.Update(restriction, substitution, null);

            Console.WriteLine("Final data store contents:");
            var any = links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);
            links.Each(query, link =>
            {
                Console.WriteLine(links.Format(link));
                return links.Constants.Continue;
            });
        }
    }
}