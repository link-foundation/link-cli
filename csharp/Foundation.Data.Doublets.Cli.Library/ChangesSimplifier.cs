using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    public static class ChangesSimplifier
    {
        /// <summary>
        /// Reduces the raw (before, after) steps a store reports to one change per link address:
        /// from its state before the query to its state after it.
        /// </summary>
        /// <remarks>
        /// The store reports every step it takes — a link is emptied to (i: 0 0) before it is deleted
        /// and created as (i: 0 0) before it is set — and the null link (0: 0 0) stands for "no link",
        /// before a creation and after a deletion. The steps of one address form a chain, so its net
        /// change is the before of its first step and the after of its last:
        /// a link created and deleted again within the query is not reported, and a link that ends
        /// where it started is reported unchanged, like a link the query only matched.
        /// The changes are ordered by their after state, so deletions come first; changes with equal
        /// after states keep the order of their first step.
        /// </remarks>
        public static IEnumerable<(Link<uint> Before, Link<uint> After)> SimplifyChanges(
            IEnumerable<(Link<uint> Before, Link<uint> After)> changes
        )
        {
            ArgumentNullException.ThrowIfNull(changes);

            var netChanges = new List<(Link<uint> Before, Link<uint> After)>();
            var positionOfAddress = new Dictionary<uint, int>();
            foreach (var (before, after) in changes)
            {
                var address = before.Index != 0 ? before.Index : after.Index;
                if (address == 0)
                {
                    continue;
                }
                if (positionOfAddress.TryGetValue(address, out var position))
                {
                    netChanges[position] = (netChanges[position].Before, after);
                }
                else
                {
                    positionOfAddress[address] = netChanges.Count;
                    netChanges.Add((before, after));
                }
            }
            return netChanges
                .Where(change => !(IsNull(change.Before) && IsNull(change.After)))
                .OrderBy(change => change.After.Index)
                .ThenBy(change => change.After.Source)
                .ThenBy(change => change.After.Target)
                .ToList();
        }

        private static bool IsNull(Link<uint> link) => link.Index == 0 && link.Source == 0 && link.Target == 0;
    }
}
