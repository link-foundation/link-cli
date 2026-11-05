using System;
using System.Collections.Generic;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// A network made of other networks (issue #149), such as a
    /// <see cref="UnionLinksDecorator{TLinkAddress}"/> or a
    /// <see cref="RemapLinksDecorator{TLinkAddress}"/>: a link at a given address
    /// is created in the network the address is of, not by creating links in
    /// the primary one until the address comes up. <c>EnsureCreated</c> of
    /// <see cref="LinksExtensions"/> asks it, and so does every undo of a
    /// delete, such as the one of a write a permission layer refuses.
    /// </summary>
    public interface IComposedLinks<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        /// <summary>Whether <paramref name="address"/> is of this network: a remap claims the addresses of its map, a union those any member claims.</summary>
        bool Claims(TLinkAddress address);

        /// <summary>Creates a link at every address of <paramref name="addresses"/> that has none, in the network the address is of.</summary>
        void EnsureCreated(params TLinkAddress[] addresses);
    }

    /// <summary>
    /// A store that answers for a reference apart from reading (issue #161): under a
    /// <see cref="PermissionsDecorator{TLinkAddress}"/> a subject may refer to a link it may not read,
    /// and a link it may not refer to is as missing as a deleted one.
    /// </summary>
    public interface IReferencedLinks<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        /// <summary>Whether a reference of a query to <paramref name="address"/> names a link that exists.</summary>
        bool ReferenceExists(TLinkAddress address);

        /// <summary>Refuses to create a link at <paramref name="address"/> when the address may not be chosen.</summary>
        void RequireAddress(TLinkAddress address);
    }

    /// <summary>
    /// How the addresses of a network below a <see cref="RemapLinksDecorator{TLinkAddress}"/>
    /// read above it (issue #149). Only addresses of links are mapped: the null
    /// link, <c>Any</c> and the other constants of the store read the same on
    /// both sides.
    /// </summary>
    public interface IAddressMap<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>The address below of <paramref name="outer"/>; false when <paramref name="outer"/> is no address of the network below.</summary>
        bool TryToInner(TLinkAddress outer, out TLinkAddress inner);

        /// <summary>The address above of <paramref name="inner"/>, given one when it has none yet.</summary>
        TLinkAddress ToOuter(TLinkAddress inner);
    }

    /// <summary>
    /// Every address moved by the same offset: the network below reads at
    /// <c>offset + 1</c> and up, so networks moved by different offsets share no
    /// address and a <see cref="UnionLinksDecorator{TLinkAddress}"/> of them
    /// shadows nothing.
    /// </summary>
    public sealed class OffsetAddressMap<TLinkAddress> : IAddressMap<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <param name="offset">What every address of the network below is moved by.</param>
        public OffsetAddressMap(TLinkAddress offset) => Offset = offset;

        /// <summary>What every address of the network below is moved by.</summary>
        public TLinkAddress Offset { get; }

        public bool TryToInner(TLinkAddress outer, out TLinkAddress inner)
        {
            if (Comparer<TLinkAddress>.Default.Compare(outer, Offset) <= 0)
            {
                inner = TLinkAddress.Zero;
                return false;
            }
            inner = outer - Offset;
            return true;
        }

        public TLinkAddress ToOuter(TLinkAddress inner)
        {
            var outer = inner + Offset;
            if (Comparer<TLinkAddress>.Default.Compare(outer, inner) < 0)
            {
                throw new OverflowException($"Address {inner} moved by {Offset} is past the last address.");
            }
            return outer;
        }
    }

    /// <summary>
    /// A table of the addresses above and below: an address below that has
    /// none above yet gets the next free one from <c>first</c> on, so the
    /// addresses of another system (a server, a service) read as addresses of
    /// our own range. <see cref="Pairs"/> and <see cref="Add"/> keep the table
    /// across runs, in links or anywhere else.
    /// </summary>
    public sealed class AddressTable<TLinkAddress> : IAddressMap<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        private readonly Dictionary<TLinkAddress, TLinkAddress> _inner = new();
        private readonly Dictionary<TLinkAddress, TLinkAddress> _outer = new();
        private TLinkAddress _next;

        /// <param name="first">The first address above the table gives.</param>
        public AddressTable(TLinkAddress first)
        {
            if (first == TLinkAddress.Zero)
            {
                throw new ArgumentException("The null link is no address to give.", nameof(first));
            }
            _next = first;
        }

        /// <summary>Every address above with its address below.</summary>
        public IReadOnlyDictionary<TLinkAddress, TLinkAddress> Pairs => _inner;

        /// <summary>Maps <paramref name="outer"/> to <paramref name="inner"/>; neither may be mapped to another address already.</summary>
        public void Add(TLinkAddress outer, TLinkAddress inner)
        {
            if (outer == TLinkAddress.Zero || inner == TLinkAddress.Zero)
            {
                throw new ArgumentException("The null link is mapped to itself.");
            }
            if (_inner.TryGetValue(outer, out var known) && known != inner)
            {
                throw new ArgumentException($"Address {outer} is mapped to {known} already.", nameof(outer));
            }
            if (_outer.TryGetValue(inner, out known) && known != outer)
            {
                throw new ArgumentException($"Address {inner} is mapped from {known} already.", nameof(inner));
            }
            _inner[outer] = inner;
            _outer[inner] = outer;
        }

        public bool TryToInner(TLinkAddress outer, out TLinkAddress inner) => _inner.TryGetValue(outer, out inner);

        public TLinkAddress ToOuter(TLinkAddress inner)
        {
            if (_outer.TryGetValue(inner, out var outer))
            {
                return outer;
            }
            while (_inner.ContainsKey(_next))
            {
                _next++;
            }
            outer = _next++;
            Add(outer, inner);
            return outer;
        }
    }
}
