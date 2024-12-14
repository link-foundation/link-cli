using System.Numerics;
using Platform.Converters;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
  public static class LinksExtensions
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
