using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Sequences.Walkers;

namespace Foundation.Data.Doublets.Cli;

// Keep the maintained Unicode converter; validate its graph at the walker seam.
internal sealed class CheckedUnicodeSequenceWalker<TLinkAddress>(ILinks<TLinkAddress> links, Func<TLinkAddress, bool> isSymbol)
    : ISequenceWalker<TLinkAddress>
    where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>, IComparisonOperators<TLinkAddress, TLinkAddress, bool>
{
    public IEnumerable<TLinkAddress> Walk(TLinkAddress sequence)
    {
        var stack = new Stack<(TLinkAddress Address, bool Exit)>();
        var path = new HashSet<TLinkAddress>();
        stack.Push((sequence, false));
        while (stack.TryPop(out var frame))
        {
            var (address, exit) = frame;
            if (exit) { path.Remove(address); continue; }
            var parts = links.GetLink(address) ?? throw new InvalidDataException($"Sequence link {address} does not exist.");
            if (isSymbol(address)) { yield return address; continue; }
            if (!path.Add(address)) throw new InvalidDataException($"Unicode sequence cycle at link {address}.");
            stack.Push((address, true));
            stack.Push((parts[links.Constants.TargetPart], false));
            stack.Push((parts[links.Constants.SourcePart], false));
        }
    }
}
