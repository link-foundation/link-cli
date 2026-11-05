using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// Permissions on triplets (issues #149 and #152): the same rules as on
    /// doublets, judged on the triplet as a whole. The Rust counterpart is
    /// <c>link_cli::PermissionsDecorator</c> over a <c>Triplets</c> store.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The doublets of a triplet are not the subject's business. A rule on the
    /// type <c>Note</c> says nothing about the wrappers and pairs that happen to
    /// store the parts of a note, so a <see cref="PermissionsDecorator{TLinkAddress}"/>
    /// under <see cref="TripletsOverDoublets{TLinkAddress}"/> would hide them and
    /// refuse to write them. This one goes above instead, over the
    /// <see cref="ITriplets{TLinkAddress}"/> store, and asks the policy only about
    /// the outer doublet <c>(index: type …)</c>:
    /// </para>
    /// <list type="bullet">
    /// <item>a triplet the subject may not read does not exist for the subject,
    /// and neither does anything that is not a triplet;</item>
    /// <item>creating needs the right to create and to write the type;</item>
    /// <item>updating needs the right to write the triplet as it is and as it will be;</item>
    /// <item>deleting needs the right to write every triplet the delete takes
    /// along, or nothing is deleted;</item>
    /// <item>a point the subject created, still only its own type, is the
    /// subject's: that is what a substitution query reserves before it sets the type.</item>
    /// </list>
    /// </remarks>
    public class TripletsPermissionsDecorator<TLinkAddress> : PermissionsDecorator<TLinkAddress>, ITriplets<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        private readonly ITriplets<TLinkAddress> _triplets;

        /// <param name="triplets">The triplets below.</param>
        /// <param name="policy">Who may read and write what.</param>
        /// <param name="subject">The subject, asked at every call (the null link is a guest).</param>
        public TripletsPermissionsDecorator(ITriplets<TLinkAddress> triplets, IPermissionPolicy<TLinkAddress> policy, Func<TLinkAddress> subject)
            : base(triplets, policy, subject)
        {
            _triplets = triplets;
        }

        public TripletMarkers<TLinkAddress> Markers => _triplets.Markers;

        public virtual DoubletRole? RoleOf(TLinkAddress id)
        {
            return OuterOf(id) is { } link && CanRead(link) ? _triplets.RoleOf(id) : null;
        }

        public virtual Triplet<TLinkAddress>? GetTriplet(TLinkAddress index)
        {
            return _triplets.GetTriplet(index) is { } triplet && Readable(triplet) ? triplet : null;
        }

        public virtual bool IsTriplet(TLinkAddress index) => GetTriplet(index) is not null;

        public virtual TLinkAddress CreateTriplet(TLinkAddress type, TLinkAddress from, TLinkAddress to)
        {
            var zero = TLinkAddress.Zero;
            if (!Policy.CanCreate(Subject) || (type != zero && !Policy.CanWrite(Subject, new Link<TLinkAddress>(zero, type, zero))))
            {
                throw new ForbiddenException("create");
            }
            // A subject that may write a type need not read it, which leaves the
            // layer below to judge that the type exists.
            EnsurePart(from, null, "from");
            EnsurePart(to, null, "to");
            var index = _triplets.CreateTriplet(type, from, to);
            Remember(index);
            return index;
        }

        public virtual TLinkAddress CreateTripletPoint()
        {
            if (!Policy.CanCreate(Subject))
            {
                throw new ForbiddenException("create");
            }
            var index = _triplets.CreateTripletPoint();
            Remember(index);
            return index;
        }

        public virtual void CreateTripletPointAt(TLinkAddress address)
        {
            if (!Policy.CanCreate(Subject))
            {
                throw new ForbiddenException("create");
            }
            // A hidden link is not found, yet its address is taken: the point must
            // not land on it.
            if (OuterOf(address) is not null)
            {
                throw new ArgumentException($"Link not found: {address}");
            }
            _triplets.CreateTripletPointAt(address);
            Remember(address);
        }

        public virtual Triplet<TLinkAddress> UpdateTriplet(TLinkAddress index, TLinkAddress type, TLinkAddress from, TLinkAddress to)
        {
            if (GetTriplet(index) is not { } before)
            {
                throw new InvalidOperationException($"Link {index} is not a triplet.");
            }
            // Only an empty triplet has itself as its target.
            var target = from == TLinkAddress.Zero && to == TLinkAddress.Zero ? index : TLinkAddress.Zero;
            if (!Writable(before) || !CanWrite(new Link<TLinkAddress>(index, type, target)))
            {
                throw new ForbiddenException($"update {index}");
            }
            EnsurePart(from, index, "from");
            EnsurePart(to, index, "to");
            return _triplets.UpdateTriplet(index, type, from, to);
        }

        public virtual IReadOnlyList<Triplet<TLinkAddress>> DeleteTriplet(TLinkAddress index)
        {
            if (GetTriplet(index) is not { } first)
            {
                throw new InvalidOperationException($"Link {index} is not a triplet.");
            }
            if (!TripletsCascade.Of(_triplets, first).All(Writable))
            {
                throw new ForbiddenException($"delete {index}");
            }
            var deleted = _triplets.DeleteTriplet(index);
            foreach (var triplet in deleted)
            {
                Forget(triplet.Index);
            }
            return deleted;
        }

        public virtual IReadOnlyList<Triplet<TLinkAddress>> EachTriplet(TripletQuery<TLinkAddress> query)
        {
            return _triplets.EachTriplet(query).Where(Readable).ToList();
        }

        public virtual int CountTriplets(TripletQuery<TLinkAddress> query) => EachTriplet(query).Count;

        public virtual int DoubletUsages(TLinkAddress id) => _triplets.DoubletUsages(id);

        /// <summary>The outer doublet of a triplet the store below holds.</summary>
        private Link<TLinkAddress>? OuterOf(TLinkAddress index)
        {
            return index != TLinkAddress.Zero && _links.Exists(index) ? new Link<TLinkAddress>(_links.GetLink(index)) : default(Link<TLinkAddress>?);
        }

        private bool Readable(Triplet<TLinkAddress> triplet) => OuterOf(triplet.Index) is { } outer && CanRead(outer);

        private bool Writable(Triplet<TLinkAddress> triplet) => OuterOf(triplet.Index) is { } outer && CanWrite(outer);

        private void EnsurePart(TLinkAddress part, TLinkAddress? itself, string name)
        {
            if (part != TLinkAddress.Zero && !(itself is { } self && self == part) && !IsTriplet(part))
            {
                throw new ArgumentException($"The {name} part {part} is not a triplet.");
            }
        }
    }
}
