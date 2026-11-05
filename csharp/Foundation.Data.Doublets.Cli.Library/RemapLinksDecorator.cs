using System;
using System.Collections.Generic;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// A links network under other addresses (issue #149): every address that
    /// goes down is mapped to the network's own, and every address that comes up
    /// is mapped back, so the code above never thinks about remapping. The
    /// JavaScript counterpart is <c>RemapLinks</c> of
    /// <c>@link-foundation/link-cli-client</c>, whose addresses above may be
    /// strings such as UUIDs; the Rust one is <c>link_cli::RemapLinks</c>.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The map is an <see cref="IAddressMap{TLinkAddress}"/>: an
    /// <see cref="OffsetAddressMap{TLinkAddress}"/> moves a network out of the
    /// way of another, so a <see cref="UnionLinksDecorator{TLinkAddress}"/> of
    /// them shadows nothing, and an <see cref="AddressTable{TLinkAddress}"/>
    /// gives the addresses of another system addresses of our own. Strings
    /// from outside, such as UUIDs, are bound to links by
    /// <see cref="LinksExternalIds"/>.
    /// </para>
    /// <para>
    /// A read that names an address that is not of the network below finds
    /// nothing; a write that names one fails with <see cref="ArgumentException"/>.
    /// The constants of the store (the null link, <c>Any</c>) are no addresses
    /// and read the same on both sides.
    /// </para>
    /// </remarks>
    public class RemapLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, INamedTypesLinks<TLinkAddress>, IComposedLinks<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        private readonly INamedTypesLinks<TLinkAddress> _named;

        /// <param name="links">The network below.</param>
        /// <param name="map">How its addresses read above.</param>
        public RemapLinksDecorator(INamedTypesLinks<TLinkAddress> links, IAddressMap<TLinkAddress> map) : base(links)
        {
            ArgumentNullException.ThrowIfNull(map);
            _named = links;
            Map = map;
            LinksExtensions.KeepOwnFacade(links);
        }

        /// <summary>How the addresses of the network below read above.</summary>
        public IAddressMap<TLinkAddress> Map { get; }

        /// <summary>The address above of <paramref name="inner"/>; a constant reads the same.</summary>
        public TLinkAddress ToOuter(TLinkAddress inner)
        {
            if (!_constants.IsInternalReference(inner))
            {
                return inner;
            }
            var outer = Map.ToOuter(inner);
            if (!_constants.IsInternalReference(outer))
            {
                throw new OverflowException($"Address {inner} maps to {outer}, which is no address.");
            }
            return outer;
        }

        /// <summary>The address below of <paramref name="outer"/>; false when it is no address of the network below. A constant reads the same.</summary>
        public bool TryToInner(TLinkAddress outer, out TLinkAddress inner)
        {
            if (!_constants.IsInternalReference(outer))
            {
                inner = outer;
                return true;
            }
            return Map.TryToInner(outer, out inner);
        }

        /// <summary>The address below of <paramref name="outer"/>; throws when it is no address of the network below.</summary>
        public TLinkAddress ToInner(TLinkAddress outer) =>
            TryToInner(outer, out var inner) ? inner : throw new ArgumentException($"Address {outer} is not of this network.");

        /// <summary>Whether <paramref name="address"/> is mapped to an address below.</summary>
        public virtual bool Claims(TLinkAddress address) => TryToInner(address, out _);

        /// <summary>Creates the links at <paramref name="addresses"/> below, at the addresses they map to.</summary>
        public virtual void EnsureCreated(params TLinkAddress[] addresses)
        {
            ArgumentNullException.ThrowIfNull(addresses);
            var inner = new TLinkAddress[addresses.Length];
            for (var i = 0; i < addresses.Length; i++)
            {
                inner[i] = ToInner(addresses[i]);
            }
            _links.EnsureCreated(inner);
        }

        public override TLinkAddress Count(IList<TLinkAddress>? restriction) =>
            TryInner(restriction, out var inner) ? _links.Count(inner) : TLinkAddress.Zero;

        public override TLinkAddress Each(IList<TLinkAddress>? restriction, ReadHandler<TLinkAddress>? handler)
        {
            if (!TryInner(restriction, out var inner))
            {
                return _constants.Continue;
            }
            return _links.Each(inner, link => handler is null ? _constants.Continue : handler(Outer(link)));
        }

        public override TLinkAddress Create(IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler) =>
            ToOuter(_links.Create(Inner(substitution), Outer(handler)));

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler) =>
            ToOuter(_links.Update(Found(restriction), Inner(substitution), Outer(handler)));

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler) =>
            ToOuter(_links.Delete(Found(restriction), Outer(handler)));

        public virtual string? GetName(TLinkAddress link) =>
            TryToInner(link, out var inner) ? _named.GetName(inner) : null;

        public virtual TLinkAddress GetByName(string name) => ToOuter(_named.GetByName(name));

        public virtual TLinkAddress SetName(TLinkAddress link, string name) => _named.SetName(ToInner(link), name);

        public virtual void RemoveName(TLinkAddress link)
        {
            if (TryToInner(link, out var inner))
            {
                _named.RemoveName(inner);
            }
        }

        /// <summary>A link of the network below as it reads above.</summary>
        protected IList<TLinkAddress>? Outer(IList<TLinkAddress>? link)
        {
            if (link is null)
            {
                return null;
            }
            var outer = new TLinkAddress[link.Count];
            for (var i = 0; i < link.Count; i++)
            {
                outer[i] = ToOuter(link[i]);
            }
            return outer;
        }

        private WriteHandler<TLinkAddress>? Outer(WriteHandler<TLinkAddress>? handler) =>
            handler is null ? null : (before, after) => handler(Outer(before), Outer(after));

        private bool TryInner(IList<TLinkAddress>? restriction, out IList<TLinkAddress>? inner)
        {
            if (restriction is null)
            {
                inner = null;
                return true;
            }
            var mapped = new TLinkAddress[restriction.Count];
            for (var i = 0; i < restriction.Count; i++)
            {
                if (!TryToInner(restriction[i], out mapped[i]))
                {
                    inner = null;
                    return false;
                }
            }
            inner = mapped;
            return true;
        }

        private IList<TLinkAddress>? Inner(IList<TLinkAddress>? substitution) =>
            TryInner(substitution, out var inner)
                ? inner
                : throw new ArgumentException($"A link of this network cannot refer to {string.Join(" ", substitution!)}: an address is not of this network.");

        /// <summary>The restriction of a write below; a link that is not of the network below is not found.</summary>
        private IList<TLinkAddress>? Found(IList<TLinkAddress>? restriction)
        {
            if (restriction is { Count: > 0 } && !TryToInner(restriction[_constants.IndexPart], out _))
            {
                throw new ArgumentException($"Link not found: {restriction[_constants.IndexPart]}");
            }
            return Inner(restriction);
        }
    }
}
