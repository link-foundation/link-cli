﻿using System.CommandLine;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace LiNoCliTool
{
    class Program
    {
        static async Task Main(string[] args)
        {
            var dbOption = new Option<string>(
                name: "--db",
                description: "Path to the links database file",
                getDefaultValue: () => "db.links"
            );

            var queryOption = new Option<string>(
                name: "--query",
                description: "LiNo query for CRUD operation"
            );

            var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
            {
                dbOption,
                queryOption
            };

            rootCommand.SetHandler((string db, string query) =>
            {
                using var links = new UnitedMemoryLinks<uint>(db);

                if (string.IsNullOrWhiteSpace(query))
                {
                    return;
                }

                var parser = new Parser();
                var parsedLinks = parser.Parse(query);

                // Process parsed links based on CRUD operations
                ProcessLinks(links, parsedLinks);

            }, dbOption, queryOption);

            await rootCommand.InvokeAsync(args);
        }

        static void ProcessLinks(ILinks<uint> links, IList<LinoLink> parsedLinks)
        {
            static uint GetLinkAddress(ILinks<uint> links, LinoLink? link)
            {
                if (link == null)
                {
                    return links.Constants.Null;
                }

                LinoLink nonNullLink = (LinoLink)link;
                if (!string.IsNullOrEmpty(nonNullLink.Id) && uint.TryParse(nonNullLink.Id, out uint linkId))
                {
                    return linkId;
                }

                if (nonNullLink.Values != null && nonNullLink.Values.Count >= 2)
                {
                    uint source = GetLinkAddress(links, nonNullLink.Values[0]);
                    uint target = GetLinkAddress(links, nonNullLink.Values[1]);
                    if (source != links.Constants.Null && target != links.Constants.Null)
                    {
                        return links.GetOrCreate(source, target);
                    }
                }
                else if (nonNullLink.Values != null && nonNullLink.Values.Count == 1)
                {
                    return GetLinkAddress(links, nonNullLink.Values[0]);
                }
                return links.Constants.Null;
            }
            if (parsedLinks.Count == 0)
            {
                return;
            }

            var outerLink = parsedLinks[0];

            if (outerLink.Values == null || outerLink.Values.Count < 2)
            {
                return;
            }

            var restrictionLink = outerLink.Values[0];
            var substitutionLink = outerLink.Values[1];

            if ((restrictionLink.Values == null || restrictionLink.Values.Count == 0) &&
                (substitutionLink.Values == null || substitutionLink.Values.Count == 0))
            {
                return;
            }

            // If restriction is empty, perform create operation
            if (restrictionLink.Values == null || restrictionLink.Values.Count == 0)
            {
                // Process substitutionLink
                if (substitutionLink.Values != null && substitutionLink.Values.Count > 0)
                {
                    foreach (var linkToCreate in substitutionLink.Values)
                    {
                        uint linkAddress = GetLinkAddress(links, linkToCreate);
                        if (linkAddress == links.Constants.Null)
                        {
                            Console.WriteLine("Failed to create link.");
                        }
                    }
                }

                // Print the final data store contents
                var any = links.Constants.Any;
                var query = new DoubletLink(index: any, source: any, target: any);
                links.Each(query, link =>
                {
                    Console.WriteLine(links.Format(link));
                    return links.Constants.Continue;
                });
                return;
            }

            // Existing code for updates remains unchanged
            uint linkId = links.Constants.Null;
            uint restrictionSource = links.Constants.Null;
            uint restrictionTarget = links.Constants.Null;
            uint substitutionSource = links.Constants.Null;
            uint substitutionTarget = links.Constants.Null;

            // Process restrictionLink
            if (restrictionLink.Values != null && restrictionLink.Values.Count > 0)
            {
                var restrictionInnerLink = restrictionLink.Values[0];
                linkId = GetLinkAddress(links, restrictionInnerLink);

                if (linkId == links.Constants.Null)
                {
                    return;
                }

                if (restrictionInnerLink.Values != null && restrictionInnerLink.Values.Count >= 2)
                {
                    restrictionSource = GetLinkAddress(links, restrictionInnerLink.Values[0]);
                    restrictionTarget = GetLinkAddress(links, restrictionInnerLink.Values[1]);

                    if (restrictionSource == links.Constants.Null || restrictionTarget == links.Constants.Null)
                    {
                        return;
                    }
                }
                else
                {
                    return;
                }
            }
            else
            {
                return;
            }

            // Process substitutionLink
            if (substitutionLink.Values != null && substitutionLink.Values.Count > 0)
            {
                var substitutionInnerLink = substitutionLink.Values[0];

                if (substitutionInnerLink.Values != null && substitutionInnerLink.Values.Count >= 2)
                {
                    substitutionSource = GetLinkAddress(links, substitutionInnerLink.Values[0]);
                    substitutionTarget = GetLinkAddress(links, substitutionInnerLink.Values[1]);

                    if (substitutionSource == links.Constants.Null || substitutionTarget == links.Constants.Null)
                    {
                        return;
                    }
                }
                else
                {
                    return;
                }
            }
            else
            {
                return;
            }

            var restriction = new List<uint> { linkId, restrictionSource, restrictionTarget };
            var substitution = new List<uint> { linkId, substitutionSource, substitutionTarget };

            links.Update(restriction, substitution, (before, after) =>
            {
                return links.Constants.Continue;
            });

            var anyValue = links.Constants.Any;
            var queryLink = new DoubletLink(index: anyValue, source: anyValue, target: anyValue);
            links.Each(queryLink, link =>
            {
                Console.WriteLine(links.Format(link));
                return links.Constants.Continue;
            });
        }
    }
}