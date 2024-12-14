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
  public static class MixedQueryProcessor
  {
    public class Options
    {
      public string? Query { get; set; }
      public WriteHandler<uint>? ChangesHandler { get; set; }

      public static implicit operator Options(string query) => new Options { Query = query };
    }

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

      var restrictionLink = outerLinkValues![0];
      var substitutionLink = outerLinkValues![1];

      if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
      {
        return;
      }
      else if ((restrictionLink.Values?.Count > 0) && (substitutionLink.Values?.Count > 0))
      {
        // Build dictionaries
        var restrictionLinksById = (restrictionLink.Values ?? new List<LinoLink>())
            .Where(l => !string.IsNullOrEmpty(l.Id))
            .ToDictionary(l => l.Id!);
        if (!string.IsNullOrEmpty(restrictionLink.Id))
        {
          restrictionLinksById[restrictionLink.Id!] = restrictionLink;
        }

        var substitutionLinksById = (substitutionLink.Values ?? new List<LinoLink>())
            .Where(l => !string.IsNullOrEmpty(l.Id))
            .ToDictionary(l => l.Id!);
        if (!string.IsNullOrEmpty(substitutionLink.Id))
        {
          substitutionLinksById[substitutionLink.Id!] = substitutionLink;
        }

        var allIds = restrictionLinksById.Keys.Union(substitutionLinksById.Keys).ToList();

        // Collect variable assignments from restriction links
        var variableAssignments = new Dictionary<string, uint>();
        foreach (var kv in restrictionLinksById)
        {
          var lino = kv.Value;
          if (lino.Values?.Count == 2 && lino.Id != null)
          {
            // This means we have something like (2: $var1 $var2) or (2: 1 $var)
            // Get the actual DB link to resolve variables
            var dbl = ToDoubletLink(links, lino, any);
            if (dbl.Index != any && dbl.Index != @null)
            {
              var actual = new DoubletLink(links.GetLink(dbl.Index));
              // actual.Source and actual.Target contain the real numbers
              // lino.Values[0].Id and lino.Values[1].Id may contain variables
              AssignVariableFromLink(lino.Values[0].Id, actual.Source, variableAssignments, any, @null);
              AssignVariableFromLink(lino.Values[1].Id, actual.Target, variableAssignments, any, @null);
            }
          }
          else if (lino.Values?.Count == 2 && lino.Id?.StartsWith("$") != true)
          {
            // Similar logic for a link without explicit index but with variables
            var dbl = ToDoubletLink(links, lino, any);
            // If dbl.Index is unknown, we can't directly read from DB by index, but we can still assign known numeric parts
            // If source or target is numeric or '*', no assignment needed unless it's variable
            AssignVariableFromLink(lino.Values[0].Id, dbl.Source, variableAssignments, any, @null);
            AssignVariableFromLink(lino.Values[1].Id, dbl.Target, variableAssignments, any, @null);
          }
        }

        // Before comparing variables for no-op, let's apply variable substitution to substitution links
        // Replace variables in substitutionLinksById with their assigned values if any
        foreach (var kv in substitutionLinksById.ToList())
        {
          var lino = kv.Value;
          if (lino.Values?.Count == 2)
          {
            var newSourceId = ReplaceVariable(lino.Values[0].Id, variableAssignments);
            var newTargetId = ReplaceVariable(lino.Values[1].Id, variableAssignments);

            if (newSourceId != lino.Values[0].Id || newTargetId != lino.Values[1].Id)
            {
              if (lino.Id != null)
              {
                lino = new LinoLink(lino.Id, new List<LinoLink> { new LinoLink(newSourceId), new LinoLink(newTargetId) });
              }
              else
              {
                lino = new LinoLink(new List<LinoLink> { new LinoLink(newSourceId), new LinoLink(newTargetId) });
              }
              substitutionLinksById[kv.Key] = lino;
            }
          }
        }

        // Basic variable no-op check
        var variableIds = allIds.Where(id => id.StartsWith("$")).ToArray();
        foreach (var varId in variableIds)
        {
          if (restrictionLinksById.TryGetValue(varId, out var varRestrictionLink)
              && substitutionLinksById.TryGetValue(varId, out var varSubstitutionLink))
          {
            if (AreLinksEquivalent(varRestrictionLink, varSubstitutionLink))
            {
              // Remove this variable from difference tracking
              allIds = allIds.Except([varId]).ToList();
            }
          }
        }

        // After handling variables, if allIds is empty, it means no changes.
        // If we have variables, let's treat this scenario as a read operation.
        if (!allIds.Any() && variableIds.Any())
        {
          // Perform read operation for each restriction pattern link
          foreach (var kv in restrictionLinksById)
          {
            var restrictionPattern = ToDoubletLink(links, kv.Value, links.Constants.Any);
            ReadAll(links, restrictionPattern, options);
          }
          return;
        }

        // If we still have differences, proceed with sets/unsets/updates
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
              return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
            });
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

    static string ReplaceVariable(string? id, Dictionary<string, uint> variableAssignments)
    {
      if (string.IsNullOrEmpty(id)) return id ?? "";
      if (variableAssignments.TryGetValue(id, out var val))
      {
        return val.ToString();
      }
      return id;
    }

    static void AssignVariableFromLink(string? varId, uint val, Dictionary<string, uint> variableAssignments, uint any, uint @null)
    {
      if (!string.IsNullOrEmpty(varId) && varId.StartsWith("$") && val != any && val != @null)
      {
        variableAssignments[varId] = val;
      }
    }

    static bool AreLinksEquivalent(LinoLink a, LinoLink b)
    {
      if (a.Id != b.Id) return false;
      if (a.Values?.Count != b.Values?.Count) return false;
      if (a.Values == null || b.Values == null) return a.Values == b.Values;
      for (int i = 0; i < a.Values.Count; i++)
      {
        var av = a.Values[i];
        var bv = b.Values[i];
        if (av.Id != bv.Id) return false;
      }
      return true;
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

    public static void EnsureCreated<TLinkAddress>(this ILinks<TLinkAddress> links, Func<TLinkAddress> creator, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
      var addressToUInt64Converter = CheckedConverter<TLinkAddress, TLinkAddress>.Default;
      var uInt64ToAddressConverter = CheckedConverter<TLinkAddress, TLinkAddress>.Default;
      var nonExistentAddresses = new HashSet<TLinkAddress>(addresses.Where(x => !links.Exists(x)));
      if (nonExistentAddresses?.Count > 0)
      {
        var max = nonExistentAddresses.Max()!;
        max = uInt64ToAddressConverter.Convert(TLinkAddress.CreateTruncating(Math.Min(ulong.CreateTruncating(max), ulong.CreateTruncating(links.Constants.InternalReferencesRange.Maximum))));
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
