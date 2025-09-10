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
        var seenAddresses = new HashSet<TLinkAddress>();
        TLinkAddress createdLink;
        var maxIterations = 10000; // More conservative limit to prevent infinite loops
        var iterations = 0;
        
        do
        {
          createdLink = creator();
          
          // Check for infinite loop conditions first
          if (iterations++ > maxIterations)
          {
            throw new InvalidOperationException($"Link creation exceeded maximum iterations ({maxIterations}). This may indicate a circular reference or infinite recursion in the link creation process.");
          }
          
          // Early break if we're in an obvious cycle
          if (createdLinks.Count > 0 && seenAddresses.Contains(createdLink) && createdLink != max)
          {
            // If we've created many links and started seeing repeats (but not the target), likely infinite loop
            if (createdLinks.Count > 50)
            {
              throw new InvalidOperationException($"Link creation appears to be in an infinite loop. Created {createdLinks.Count} links, seeing repeated address {createdLink}, but target {max} not reached.");
            }
          }
          
          seenAddresses.Add(createdLink);
          createdLinks.Add(createdLink);
          
          // Additional safety: if we've created far more links than the target ID suggests, something is wrong
          if (createdLinks.Count > Math.Max(100, (int)(ulong.CreateTruncating(max) * 2)))
          {
            throw new InvalidOperationException($"Link creation created {createdLinks.Count} links while trying to reach {max}. This suggests infinite recursion.");
          }
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
