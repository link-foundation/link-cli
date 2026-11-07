using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    public static class LinksExtensions
    {
        /// <summary>Whether a reference of a query to <paramref name="address"/> names a link that exists: a view that hides links answers for itself.</summary>
        public static bool ReferenceExists<TLinkAddress>(this ILinks<TLinkAddress> links, TLinkAddress address) where TLinkAddress : IUnsignedNumber<TLinkAddress> =>
            links is IReferencedLinks<TLinkAddress> referenced ? referenced.ReferenceExists(address) : links.Exists(address);

        public static void EnsureCreated<TLinkAddress>(this ILinks<TLinkAddress> links, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress>
        {
            foreach (var address in addresses) EnsureSupportedInternalReference(links, address);
            if (links is IReferencedLinks<TLinkAddress> referenced)
            {
                foreach (var address in addresses)
                {
                    if (!links.Exists(address))
                    {
                        referenced.RequireAddress(address);
                    }
                }
            }
            if (links is IComposedLinks<TLinkAddress> composed)
            {
                // A union or a remap creates a link in the network its address is of.
                composed.EnsureCreated(addresses);
                return;
            }
            if (links is INamedTypesLinks<TLinkAddress> named)
            {
                var fallback = addresses.Where(address => !named.TryEnsureCreated(address)).ToArray();
                links.EnsureCreated(links.Create, fallback);
            }
            else links.EnsureCreated(links.Create, addresses);
        }

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

                    // A store hands out the address freed last first, which can lie beyond the target. An
                    // address beyond every link the store held is appended instead, and so is every one after it.
                    if (Comparer<TLinkAddress>.Default.Compare(createdLink, max) > 0 && !IsBelowAStoredLink(links, createdLink, seenCreatedLinks))
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

        private static bool IsBelowAStoredLink<TLinkAddress>(ILinks<TLinkAddress> links, TLinkAddress address, ISet<TLinkAddress> createdLinks) where TLinkAddress : IUnsignedNumber<TLinkAddress>
        {
            var constants = links.Constants;
            var found = false;
            links.Each([constants.Any], link =>
            {
                var index = link![constants.IndexPart];
                found = Comparer<TLinkAddress>.Default.Compare(index, address) > 0 && !createdLinks.Contains(index);
                return found ? constants.Break : constants.Continue;
            });
            return found;
        }

        private static void EnsureSupportedInternalReference<TLinkAddress>(ILinks<TLinkAddress> links, TLinkAddress address) where TLinkAddress : IUnsignedNumber<TLinkAddress>
        {
            if (!links.Constants.IsInternalReference(address))
            {
                throw new InvalidOperationException($"Cannot ensure unsupported link address {address}. Only non-zero internal references in the supported range can be created.");
            }
        }

        /// <summary>
        /// Gives <paramref name="links"/> back its own facade. A decorator makes
        /// itself the facade of every decorator under it, which those call for
        /// the writes a write leads to (the usages a merge repoints, the links a
        /// delete deletes too); a layer that is not a plain pass-through (a view
        /// as a subject sees it, a union of networks, a remap of addresses) keeps
        /// those writes in the store they belong to.
        /// </summary>
        internal static void KeepOwnFacade<TLinkAddress>(ILinks<TLinkAddress> links) where TLinkAddress : IUnsignedNumber<TLinkAddress>
        {
            if (links is Platform.Data.Doublets.Decorators.LinksDecoratorBase<TLinkAddress> decorator)
            {
                decorator.Facade = decorator;
            }
        }
    }
}
