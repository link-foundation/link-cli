using System.Numerics;
using Platform.Data.Doublets;
using Platform.Data;

namespace Foundation.Data.Doublets.Cli
{
    public interface INamedTypes<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        string? GetName(TLinkAddress link);
        TLinkAddress SetName(TLinkAddress link, string name);
        TLinkAddress GetByName(string name);
        void RemoveName(TLinkAddress link);
    }

    public interface INamedTypesLinks<TLinkAddress> : ILinks<TLinkAddress>, INamedTypes<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        /// <summary>Whether assigning a name removes its previous owner.</summary>
        bool ReassignsNames => true;

        /// <summary>A detached logical name snapshot for undo. Stores supporting names of absent
        /// rows override this default row-based enumeration to include them.</summary>
        IReadOnlyDictionary<TLinkAddress, string> CaptureNames()
        {
            var names = new Dictionary<TLinkAddress, string>();
            foreach (var row in this.All(new Link<TLinkAddress>(Constants.Any, Constants.Any, Constants.Any)))
            {
                var index = new Link<TLinkAddress>(row).Index;
                if (GetName(index) is { } name) names[index] = name;
            }
            return names;
        }
    }
}
