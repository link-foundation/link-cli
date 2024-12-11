using Platform.Data.Doublets;
using Platform.Protocols.Lino;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using Platform.Converters;
using System.Numerics;
using Platform.Data;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli
{
  // Query Processor class with single static method to process queries
  public static class MixedQueryProcessor
  {
    public class Options
    {
      public string? Query { get; set; }

      public WriteHandler<uint>? ChangesHandler { get; set; }

      // implicit conversion from string to Options

      public static implicit operator Options(string query) => new Options { Query = query };
    }

    // ProcessQuery method to process queries
    public static void ProcessQuery(ILinks<uint> links, Options options)
    {
      var query = options.Query;

      var @null = links.Constants.Null;
      var any = links.Constants.Any;

      if (string.IsNullOrEmpty(query))
      {
        return;
      }

      var parser = new Parser();
      var parsedLinks = parser.Parse(query);

      if (parsedLinks.Count == 0)
      {
        return;
      }

      var outerLink = parsedLinks[0];
      var outerLinkValues = outerLink.Values;

      if (outerLinkValues?.Count < 2)
      {
        return;
      }

      if (outerLinkValues == null)
      {
        return;
      }

      var restrictionLink = outerLinkValues[0];
      var substitutionLink = outerLinkValues[1];

      if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
      {
        return;
      }
      else if ((restrictionLink.Values?.Count > 0) && (substitutionLink.Values?.Count > 0))
      {
        // Build dictionaries mapping IDs to links
        var restrictionLinksById = restrictionLink.Values?
            .Where(linoLink => !string.IsNullOrEmpty(linoLink.Id))
            .ToDictionary(linoLink => linoLink.Id);

        var substitutionLinksById = substitutionLink.Values?
            .Where(linoLink => !string.IsNullOrEmpty(linoLink.Id))
            .ToDictionary(linoLink => linoLink.Id);

        // Handle null dictionaries
        restrictionLinksById ??= new Dictionary<string, LinoLink>();
        substitutionLinksById ??= new Dictionary<string, LinoLink>();

        var allIds = restrictionLinksById.Keys.Union(substitutionLinksById.Keys);

        foreach (var id in allIds)
        {
          bool hasRestriction = restrictionLinksById.TryGetValue(id, out var restrictionLinoLink);
          bool hasSubstitution = substitutionLinksById.TryGetValue(id, out var substitutionLinoLink);

          if (hasRestriction && hasSubstitution)
          {
            // Update operation
            var restrictionDoublet = ToDoubletLink(links, restrictionLinoLink, any);
            var substitutionDoublet = ToDoubletLink(links, substitutionLinoLink, @null);

            if (restrictionDoublet.Index == @any && substitutionDoublet.Index == @any
              && restrictionDoublet.Source == @any && substitutionDoublet.Source == @any
              && restrictionDoublet.Target == @any && substitutionDoublet.Target == @any)
            {
              ReadAll(links, restrictionDoublet, options);
            }
            else
            {
              links.Update(restrictionDoublet, substitutionDoublet, (before, after) =>
              {
                return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
              });
            }
          }
          else if (hasRestriction && !hasSubstitution)
          {
            var queryLink = ToDoubletLink(links, restrictionLinoLink, any);
            Unset(links, queryLink, options);
          }
          else if (!hasRestriction && hasSubstitution)
          {
            var doubletLink = ToDoubletLink(links, substitutionLinoLink, @null);
            Set(links, doubletLink, options);
          }
        }

        return;
      }
      else if (substitutionLink.Values?.Count == 0) // If substitution is empty, perform delete operation
      {
        foreach (var linkToDelete in restrictionLink.Values ?? [])
        {
          var queryLink = ToDoubletLink(links, linkToDelete, any);
          Unset(links, queryLink, options);
        }
        return;
      }
      else if (restrictionLink.Values?.Count == 0) // If restriction is empty, perform create operation
      {
        foreach (var linkToCreate in substitutionLink.Values ?? [])
        {
          var doubletLink = ToDoubletLink(links, linkToCreate, @null);
          Set(links, doubletLink, options);
        }
        return;
      }
    }

    static void Set(this ILinks<uint> links, DoubletLink substitutionLink, Options options)
    {
      var @null = links.Constants.Null;
      var any = links.Constants.Any;
      if (substitutionLink.Source == any)
      {
        throw new ArgumentException($"The source of the link {substitutionLink} cannot be any.");
      }
      if (substitutionLink.Target == any)
      {
        throw new ArgumentException($"The target of the link {substitutionLink} cannot be any.");
      }
      if (substitutionLink.Index != @null)
      {
        // links.EnsureCreated(doubletLink.Index);
        MixedLinksExtensions.EnsureCreated(links, substitutionLink.Index); // contain fix
        var restrictionDoublet = new DoubletLink(substitutionLink.Index, any, any);
        options.ChangesHandler?.Invoke(null, restrictionDoublet);
        links.Update(restrictionDoublet, substitutionLink, (before, after) =>
        {
          return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
        });
      }
      else
      {
        // Get or create
        var linkIndex = links.SearchOrDefault(substitutionLink.Source, substitutionLink.Target);

        if (linkIndex == default)
        {
          linkIndex = links.CreateAndUpdate(substitutionLink.Source, substitutionLink.Target, (before, after) =>
          {
            return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
          });

          // var newLink = new DoubletLink(linkIndex, doubletLink.Source, doubletLink.Target);
          // options.ChangesHandler?.Invoke(null, newLink);
        }
        else
        {
          var existingLink = new DoubletLink(linkIndex, substitutionLink.Source, substitutionLink.Target);
          options.ChangesHandler?.Invoke(existingLink, existingLink);
        }
      }
    }

    static void ReadAll(this ILinks<uint> links, DoubletLink restrictionLink, Options options)
    {
      links.Each(restrictionLink, link =>
      {
        return options.ChangesHandler?.Invoke(link, link) ?? links.Constants.Continue;
      });
    }

    static void Unset(this ILinks<uint> links, DoubletLink restrictionLink, Options options)
    {
      var linksToDelete = links.All(restrictionLink);
      foreach (var link in linksToDelete)
      {
        links.Delete(link, (before, after) =>
        {
          return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
        });
      }
    }

    static DoubletLink ToDoubletLink(ILinks<uint> links, LinoLink linoLink, uint defaultValue)
    {
      uint index = defaultValue;
      uint source = defaultValue;
      uint target = defaultValue;
      TryParseLinkId(linoLink.Id, links.Constants, ref index);
      if (linoLink.Values?.Count == 2)
      {
        var sourceLink = linoLink.Values[0];
        TryParseLinkId(sourceLink.Id, links.Constants, ref source);
        var targetLink = linoLink.Values[1];
        TryParseLinkId(targetLink.Id, links.Constants, ref target);
      }
      return new DoubletLink(index, source, target);
    }

    static void TryParseLinkId(string? id, LinksConstants<uint> constants, ref uint parsedValue)
    {
      if (string.IsNullOrEmpty(id))
      {
        return;
      }
      if (id == "*")
      {
        parsedValue = constants.Any;
      }
      else if (uint.TryParse(id, out uint linkId))
      {
        parsedValue = linkId;
      }
    }
  }

  public static class MixedLinksExtensions
  {

    public static void EnsureCreated<TLinkAddress>(this ILinks<TLinkAddress> links, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress> { links.EnsureCreated(links.Create, addresses); }


    public static void EnsurePointsCreated<TLinkAddress>(this ILinks<TLinkAddress> links, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress> { links.EnsureCreated(links.CreatePoint, addresses); }


    public static void EnsureCreated<TLinkAddress>(this ILinks<TLinkAddress> links, Func<TLinkAddress> creator, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
      var addressToUInt64Converter = CheckedConverter<TLinkAddress, TLinkAddress>.Default;
      var uInt64ToAddressConverter = CheckedConverter<TLinkAddress, TLinkAddress>.Default;
      var nonExistentAddresses = new HashSet<TLinkAddress>(addresses.Where(x => !links.Exists(x)));
      if (nonExistentAddresses.Count > 0)
      {
        var max = nonExistentAddresses.Max();
        max = uInt64ToAddressConverter.Convert(TLinkAddress.CreateTruncating(System.Math.Min(ulong.CreateTruncating(max), ulong.CreateTruncating(links.Constants.InternalReferencesRange.Maximum))));
        var createdLinks = new List<TLinkAddress>();
        TLinkAddress createdLink;
        do
        {
          createdLink = creator();
          createdLinks.Add(createdLink);
        }
        while (createdLink != max);
        for (var i = 0; i < createdLinks.Count; i++)
        {
          if (!nonExistentAddresses.Contains(createdLinks[i]))
          {
            links.Delete(createdLinks[i]);
          }
        }
      }
    }
  }
}


