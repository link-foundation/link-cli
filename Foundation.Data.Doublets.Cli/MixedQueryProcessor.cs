using Platform.Data.Doublets;
using Platform.Protocols.Lino;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using Platform.Converters;
using System.Numerics;
using Platform.Data;

namespace Foundation.Data.Doublets.Cli
{
  // Query Processor class with single static method to process queries
  public static class MixedQueryProcessor
  {
    // ProcessQuery method to process queries
    public static void ProcessQuery(ILinks<uint> links, string query)
    {
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

      var @null = links.Constants.Null;
      var any = links.Constants.Any;

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

            links.Update(restrictionDoublet, substitutionDoublet, (before, after) =>
            {
              return links.Constants.Continue;
            });
          }
          else if (hasRestriction && !hasSubstitution)
          {
            var queryLink = ToDoubletLink(links, restrictionLinoLink, any);
            links.DeleteByQuery(queryLink);
          }
          else if (!hasRestriction && hasSubstitution)
          {
            var doubletLink = ToDoubletLink(links, substitutionLinoLink, @null);
            Set(links, doubletLink);
          }
        }

        return;
      }
      else if (substitutionLink.Values?.Count == 0) // If substitution is empty, perform delete operation
      {
        foreach (var linkToDelete in restrictionLink.Values ?? [])
        {
          var queryLink = ToDoubletLink(links, linkToDelete, any);
          links.DeleteByQuery(queryLink);
        }
        return;
      }
      else if (restrictionLink.Values?.Count == 0) // If restriction is empty, perform create operation
      {
        foreach (var linkToCreate in substitutionLink.Values ?? [])
        {
          var doubletLink = ToDoubletLink(links, linkToCreate, @null);
          Set(links, doubletLink);
        }
        return;
      }
    }

    static void Set(this ILinks<uint> links, DoubletLink doubletLink)
    {
      var @null = links.Constants.Null;
      var any = links.Constants.Any;
      if (doubletLink.Index != @null)
      {
        // links.EnsureCreated(doubletLink.Index);
        MixedLinksExtensions.EnsureCreated(links, doubletLink.Index); // contain fix
        var restrictionDoublet = new DoubletLink(doubletLink.Index, any, any);
        links.Update(restrictionDoublet, doubletLink, (before, after) =>
        {
          return links.Constants.Continue;
        });
      }
      else
      {
        links.GetOrCreate(doubletLink.Source, doubletLink.Target);
      }
    }

    static DoubletLink ToDoubletLink(ILinks<uint> links, LinoLink linoLink, uint defaultValue)
    {
      uint index = defaultValue;
      uint source = defaultValue;
      uint target = defaultValue;
      if (!string.IsNullOrEmpty(linoLink.Id) && uint.TryParse(linoLink.Id, out uint linkId))
      {
        index = linkId;
      }
      if (linoLink.Values?.Count == 2)
      {
        var sourceLink = linoLink.Values[0];
        var targetLink = linoLink.Values[1];
        if (!string.IsNullOrEmpty(sourceLink.Id) && uint.TryParse(sourceLink.Id, out uint sourceId))
        {
          source = sourceId;
        }
        if (!string.IsNullOrEmpty(targetLink.Id) && uint.TryParse(targetLink.Id, out uint targetId))
        {
          target = targetId;
        }
      }
      return new DoubletLink(index, source, target);
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


