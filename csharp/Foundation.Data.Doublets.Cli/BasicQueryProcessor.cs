using Platform.Data.Doublets;
using Platform.Protocols.Lino;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli
{
  // Query Processor class with single static method to process queries
  public static class BasicQueryProcessor
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

      if ((restrictionLink.Values?.Count == 0) &&
          (substitutionLink.Values?.Count == 0))
      {
        return;
      }
      else if ((restrictionLink.Values?.Count > 0) &&
               (substitutionLink.Values?.Count > 0))
      {
        // Update operation for multiple links
        if (restrictionLink.Values.Count != substitutionLink.Values.Count)
        {
          Console.WriteLine("The number of restriction links and substitution links must be the same.");
          return;
        }

        for (int i = 0; i < restrictionLink.Values.Count; i++)
        {
          var restrictionLinoLink = restrictionLink.Values[i];
          var substitutionLinoLink = substitutionLink.Values[i];

          var restrictionDoublet = ToDoubletLink(links, restrictionLinoLink, any);
          var substitutionDoublet = ToDoubletLink(links, substitutionLinoLink, @null);

          links.Update(restrictionDoublet, substitutionDoublet, (before, after) =>
          {
            return links.Constants.Continue;
          });
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
          links.GetOrCreate(doubletLink.Source, doubletLink.Target);
        }
        return;
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
}

