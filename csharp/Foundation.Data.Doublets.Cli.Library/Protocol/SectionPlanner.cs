namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>Splits links into <see cref="Section"/>s with a linear dynamic program.</summary>
/// <remarks>
/// After link k, <c>costs[s]</c> is the fewest estimated bytes for links
/// 0..=k with link k in a section of state s. A link either continues the
/// section of the previous link (same state, no hole between them and, for a
/// fixed arity, the same length) or opens a new section after the cheapest
/// previous state, paying for a section header. The tie-breaking matches the
/// Rust <c>SectionPlanner</c>, so both ports produce identical bytes.
/// </remarks>
internal sealed class SectionPlanner
{
    /// <summary>Estimated bytes of a section header (shape and count), used to weigh a split.</summary>
    private const ulong SectionHeaderEstimate = 2;
    /// <summary>Estimated extra bytes of a variable-arity section: its extra_arity field and each length prefix.</summary>
    private const ulong VariableArityEstimate = 1;
    private const ulong Unreachable = ulong.MaxValue / 4;
    /// <summary>Planner states: a width code (0..4) times fixed (0) or variable (1) arity.</summary>
    private static readonly int States = LinksPacket.Widths.Count * 2;

    private readonly IReadOnlyList<(ulong Address, PacketReference[] References)> _links;
    private readonly byte[] _needs;

    public SectionPlanner(bool externalReferences, IReadOnlyList<(ulong Address, PacketReference[] References)> links)
    {
        _links = links;
        _needs = new byte[links.Count];
        var previousAddress = 0UL;
        for (var index = 0; index < links.Count; index++)
        {
            var (address, references) = links[index];
            if (address <= previousAddress)
            {
                throw LinoProtocolException.Unencodable(
                    $"link addresses must ascend from 1, got {address} after {previousAddress}");
            }
            if (references.Length == 0)
            {
                throw LinoProtocolException.Unencodable($"link {address} has no references");
            }
            previousAddress = address;
            byte need = 1;
            foreach (var reference in references)
            {
                need = Math.Max(need, LinksPacket.ReferenceWidth(reference, externalReferences));
            }
            _needs[index] = need;
        }
    }

    private static int FirstCheapest(ulong[] costs)
    {
        var best = 0;
        for (var state = 1; state < costs.Length; state++)
        {
            if (costs[state] < costs[best])
            {
                best = state;
            }
        }
        return best;
    }

    /// <summary>
    /// The sections as <c>(link count, width)</c> pairs; with
    /// <paramref name="packedWidths"/> every width may be used, otherwise only
    /// the widest one needed.
    /// </summary>
    public List<(int Count, byte Width)> Plan(bool packedWidths)
    {
        var sections = new List<(int Count, byte Width)>();
        if (_links.Count == 0)
        {
            return sections;
        }
        var widest = _needs.Max();
        var opensSection = new byte[_links.Count];
        var previousBest = new byte[_links.Count];
        var costs = Enumerable.Repeat(Unreachable, States).ToArray();
        for (var index = 0; index < _links.Count; index++)
        {
            var (address, link) = _links[index];
            var best = FirstCheapest(costs);
            previousBest[index] = (byte)best;
            var cheapestBefore = index == 0 ? 0 : costs[best];
            var continues = index > 0 && _links[index - 1].Address + 1 == address;
            var sameLength = index > 0 && _links[index - 1].References.Length == link.Length;
            var next = Enumerable.Repeat(Unreachable, States).ToArray();
            for (var state = 0; state < States; state++)
            {
                var width = LinksPacket.Widths[state / 2];
                var variable = state % 2 == 1;
                if (!(packedWidths || width == widest) || width < _needs[index])
                {
                    continue;
                }
                var variableEstimate = variable ? VariableArityEstimate : 0;
                var body = (ulong)link.Length * width + variableEstimate;
                var openingCost = cheapestBefore + SectionHeaderEstimate + variableEstimate;
                var continuingCost = continues && (variable || sameLength) ? costs[state] : Unreachable;
                if (continuingCost <= openingCost)
                {
                    next[state] = continuingCost + body;
                }
                else
                {
                    next[state] = openingCost + body;
                    opensSection[index] |= (byte)(1 << state);
                }
            }
            costs = next;
        }
        var current = FirstCheapest(costs);
        var end = _links.Count;
        for (var index = _links.Count - 1; index >= 0; index--)
        {
            if ((opensSection[index] & (1 << current)) != 0)
            {
                sections.Add((end - index, LinksPacket.Widths[current / 2]));
                end = index;
                current = previousBest[index];
            }
        }
        sections.Reverse();
        return sections;
    }
}
