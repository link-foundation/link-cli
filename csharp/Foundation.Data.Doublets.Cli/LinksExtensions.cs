using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
  public static class LinksExtensions
  {
    public static void EnsureCreated<TLinkAddress>(this ILinks<TLinkAddress> links, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress> { links.EnsureCreated(links.Create, addresses); }

    public static void EnsureCreated<TLinkAddress>(this ILinks<TLinkAddress> links, Func<TLinkAddress> creator, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
      var nonExistentAddresses = new HashSet<TLinkAddress>();
      foreach (var address in addresses)
      {
        EnsureSupportedInternalReference(links, address);
        if (!links.Exists(address))
        {
          nonExistentAddresses.Add(address);
        }
      }

      if (nonExistentAddresses.Count > 0)
      {
        var max = nonExistentAddresses.Max()!;
        var createdLinks = new List<TLinkAddress>();
        var seenCreatedLinks = new HashSet<TLinkAddress>();
        TLinkAddress createdLink;

        do
        {
          createdLink = creator();
          EnsureSupportedInternalReference(links, createdLink);

          if (!seenCreatedLinks.Add(createdLink))
          {
            throw new InvalidOperationException($"Link creation returned address {createdLink} more than once before reaching target {max}.");
          }

          if (Comparer<TLinkAddress>.Default.Compare(createdLink, max) > 0)
          {
            throw new InvalidOperationException($"Link creation produced address {createdLink} beyond requested target {max}.");
          }

          createdLinks.Add(createdLink);
        }
        while (createdLink != max);

        for (var i = 0; i < createdLinks.Count; i++)
        {
          if (!nonExistentAddresses.Contains(createdLinks[i]) && links.Exists(createdLinks[i]))
          {
            links.Delete(createdLinks[i]);
          }
        }
      }
    }

    private static void EnsureSupportedInternalReference<TLinkAddress>(ILinks<TLinkAddress> links, TLinkAddress address) where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
      if (!links.Constants.IsInternalReference(address))
      {
        throw new InvalidOperationException($"Cannot ensure unsupported link address {address}. Only non-zero internal references in the supported range can be created.");
      }
    }
  }
}
