using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// Many links networks seen as one (issue #149): the joining decorator of
    /// a client's links with a server's, of a cache with its source, of the
    /// networks of several services. The JavaScript counterpart is
    /// <c>UnionLinks</c> of <c>@link-foundation/link-cli-client</c>, the Rust one
    /// <c>link_cli::UnionLinks</c>.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The members are in order, and the link at an address is the one of the
    /// first member that has it: a member shadows the ones after it, as a local
    /// overlay of minimongo or minilinks shadows the documents of the server.
    /// Reads see the links of every member that are not shadowed. Members whose
    /// addresses do not overlap (a <see cref="RemapLinksDecorator{TLinkAddress}"/>
    /// with an <see cref="OffsetAddressMap{TLinkAddress}"/> moves one out of the
    /// way of another) shadow nothing.
    /// </para>
    /// <para>
    /// A new link is created in the first member, the primary one. An update,
    /// a delete and naming go to the member that has the link, so a link stays
    /// where it lives; the store of that member resolves the write, merges and
    /// cascades included, which keep to that member: a doublet is unique within
    /// its member, not across them.
    /// </para>
    /// <para>
    /// A link created at a given address (<c>EnsureCreated</c>, the undo of a
    /// delete) is created in the first member that claims the address, as a
    /// remap claims the addresses of its map, else in the primary member: see
    /// <see cref="IComposedLinks{TLinkAddress}"/>.
    /// </para>
    /// <para>
    /// A link may refer to links of other members: a link of the client typed
    /// by a type of the server holds the address of the server's type. Names
    /// are found in the members in order, so a name of the primary member wins.
    /// </para>
    /// <para>
    /// The union depends on no other layer of this library. A
    /// <see cref="PermissionsDecorator{TLinkAddress}"/> over it restricts every
    /// member at once, its rules and types coming from any of them, and one
    /// under it restricts a single member.
    /// </para>
    /// </remarks>
    public class UnionLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, INamedTypesLinks<TLinkAddress>, IComposedLinks<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <param name="members">The networks, the primary one first.</param>
        public UnionLinksDecorator(params INamedTypesLinks<TLinkAddress>[] members)
            : this((IReadOnlyList<INamedTypesLinks<TLinkAddress>>)members)
        {
        }

        /// <param name="members">The networks, the primary one first.</param>
        public UnionLinksDecorator(IReadOnlyList<INamedTypesLinks<TLinkAddress>> members)
            : base(PrimaryOf(members))
        {
            Members = members.ToArray();
            LinksExtensions.KeepOwnFacade(Members[0]);
        }

        /// <summary>The networks, the primary one first.</summary>
        public IReadOnlyList<INamedTypesLinks<TLinkAddress>> Members { get; }

        /// <summary>The network new links are created in.</summary>
        public INamedTypesLinks<TLinkAddress> Primary => Members[0];

        /// <summary>The position of the member the link at <paramref name="index"/> is of, -1 when no member has it.</summary>
        public virtual int OwnerOf(TLinkAddress index)
        {
            if (!_constants.IsInternalReference(index))
            {
                return -1;
            }
            for (var i = 0; i < Members.Count; i++)
            {
                if (Members[i].Exists(index))
                {
                    return i;
                }
            }
            return -1;
        }

        /// <summary>Whether a member claims <paramref name="address"/>; a member that is no <see cref="IComposedLinks{TLinkAddress}"/> claims every address.</summary>
        public virtual bool Claims(TLinkAddress address) =>
            Members.Any(member => member is not IComposedLinks<TLinkAddress> composed || composed.Claims(address));

        /// <summary>
        /// Creates the links at <paramref name="addresses"/> that no member has,
        /// each in the first member that is an <see cref="IComposedLinks{TLinkAddress}"/>
        /// claiming its address (a remap whose map has it), else in the primary one.
        /// </summary>
        public virtual void EnsureCreated(params TLinkAddress[] addresses)
        {
            ArgumentNullException.ThrowIfNull(addresses);
            foreach (var group in addresses.Where(address => OwnerOf(address) < 0).GroupBy(PlaceOf))
            {
                Members[group.Key].EnsureCreated(group.ToArray());
            }
        }

        /// <summary>The position of the member a new link at <paramref name="address"/> is created in.</summary>
        protected virtual int PlaceOf(TLinkAddress address)
        {
            for (var i = 0; i < Members.Count; i++)
            {
                if (Members[i] is IComposedLinks<TLinkAddress> composed && composed.Claims(address))
                {
                    return i;
                }
            }
            return 0;
        }

        public override TLinkAddress Count(IList<TLinkAddress>? restriction)
        {
            var count = TLinkAddress.Zero;
            Each(restriction, _ =>
            {
                count++;
                return _constants.Continue;
            });
            return count;
        }

        public override TLinkAddress Each(IList<TLinkAddress>? restriction, ReadHandler<TLinkAddress>? handler)
        {
            for (var i = 0; i < Members.Count; i++)
            {
                var member = i;
                var result = Members[i].Each(restriction, link =>
                    Shadowed(member, link![_constants.IndexPart]) || handler is null ? _constants.Continue : handler(link));
                if (result == _constants.Break)
                {
                    return _constants.Break;
                }
            }
            return _constants.Continue;
        }

        public override TLinkAddress Create(IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler) =>
            Primary.Create(substitution, handler);

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler) =>
            Owner(IndexOf(restriction, "update")).Update(restriction, substitution, handler);

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler) =>
            Owner(IndexOf(restriction, "delete")).Delete(restriction, handler);

        /// <summary>The name of <paramref name="link"/> in the member that has it, else in the first member that names it.</summary>
        public virtual string? GetName(TLinkAddress link)
        {
            var owner = OwnerOf(link);
            if (owner >= 0)
            {
                return Members[owner].GetName(link);
            }
            foreach (var member in Members)
            {
                if (member.GetName(link) is { } name)
                {
                    return name;
                }
            }
            return null;
        }

        /// <summary>The link of <paramref name="name"/> in the first member that has a link of that name it does not shadow.</summary>
        public virtual TLinkAddress GetByName(string name)
        {
            for (var i = 0; i < Members.Count; i++)
            {
                var link = Members[i].GetByName(name);
                if (link != _constants.Null && !Shadowed(i, link))
                {
                    return link;
                }
            }
            return _constants.Null;
        }

        /// <summary>Names <paramref name="link"/> in the member that has it, the primary one when none does.</summary>
        public virtual TLinkAddress SetName(TLinkAddress link, string name) => Owner(link).SetName(link, name);

        /// <summary>Unnames <paramref name="link"/> in the member that has it, the primary one when none does.</summary>
        public virtual void RemoveName(TLinkAddress link) => Owner(link).RemoveName(link);

        /// <summary>Whether a member before the one at <paramref name="member"/> has a link at <paramref name="index"/>.</summary>
        protected bool Shadowed(int member, TLinkAddress index)
        {
            for (var i = 0; i < member; i++)
            {
                if (Members[i].Exists(index))
                {
                    return true;
                }
            }
            return false;
        }

        private INamedTypesLinks<TLinkAddress> Owner(TLinkAddress index)
        {
            var owner = OwnerOf(index);
            return owner >= 0 ? Members[owner] : Primary;
        }

        private TLinkAddress IndexOf(IList<TLinkAddress>? restriction, string what)
        {
            var index = restriction is { Count: > 0 } ? restriction[_constants.IndexPart] : _constants.Any;
            if (index == _constants.Any || index == _constants.Null)
            {
                throw new NotSupportedException($"A union cannot {what} many links at once: give the address of the link.");
            }
            return index;
        }

        private static INamedTypesLinks<TLinkAddress> PrimaryOf(IReadOnlyList<INamedTypesLinks<TLinkAddress>> members)
        {
            ArgumentNullException.ThrowIfNull(members);
            if (members.Count == 0)
            {
                throw new ArgumentException("A union needs at least one network.", nameof(members));
            }
            if (members.Any(member => member is null))
            {
                throw new ArgumentException("A member of a union cannot be null.", nameof(members));
            }
            return members[0];
        }
    }
}
