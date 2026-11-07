using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>A triplet <c>(index: type from to)</c>; <c>From</c> and <c>To</c> are the null link when empty.</summary>
    public readonly record struct Triplet<TLinkAddress>(TLinkAddress Index, TLinkAddress Type, TLinkAddress From, TLinkAddress To)
        : IComparable<Triplet<TLinkAddress>>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>By index, then type, from and to, as the derived order of the Rust <c>Triplet</c>.</summary>
        public int CompareTo(Triplet<TLinkAddress> other)
        {
            var comparer = Comparer<TLinkAddress>.Default;
            var result = comparer.Compare(Index, other.Index);
            if (result == 0) result = comparer.Compare(Type, other.Type);
            if (result == 0) result = comparer.Compare(From, other.From);
            if (result == 0) result = comparer.Compare(To, other.To);
            return result;
        }

        public override string ToString() => $"({Index}: {Type} {From} {To})";
    }

    /// <summary>
    /// The two marker points of the encoding: <c>Pair</c>, the source of every
    /// wrapper <c>(Pair f)</c>, and <c>Nothing</c>, which stands for the empty
    /// part inside wrappers and pairs.
    /// </summary>
    public readonly record struct TripletMarkers<TLinkAddress>(TLinkAddress Pair, TLinkAddress Nothing)
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>Whether <paramref name="id"/> is one of the markers.</summary>
        public bool Contains(TLinkAddress id) => id == Pair || id == Nothing;

        internal TLinkAddress Encode(TLinkAddress part) => part == TLinkAddress.Zero ? Nothing : part;

        internal TLinkAddress Decode(TLinkAddress part) => part == Nothing ? TLinkAddress.Zero : part;
    }

    /// <summary>
    /// A restriction of <see cref="ITriplets{TLinkAddress}.EachTriplet"/>: <c>null</c>
    /// matches any value, the null link an empty <c>From</c> or <c>To</c>.
    /// </summary>
    public readonly record struct TripletQuery<TLinkAddress>(TLinkAddress? Index, TLinkAddress? Type, TLinkAddress? From, TLinkAddress? To)
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>Every triplet.</summary>
        public static TripletQuery<TLinkAddress> Any => default;

        public static TripletQuery<TLinkAddress> ByIndex(TLinkAddress index) => new(index, null, null, null);

        public static TripletQuery<TLinkAddress> ByType(TLinkAddress type) => new(null, type, null, null);

        public static TripletQuery<TLinkAddress> ByFrom(TLinkAddress from) => new(null, null, from, null);

        public static TripletQuery<TLinkAddress> ByTo(TLinkAddress to) => new(null, null, null, to);

        /// <summary>Whether <paramref name="triplet"/> satisfies every part of the restriction.</summary>
        public bool Matches(Triplet<TLinkAddress> triplet)
        {
            return (Index is not { } index || index == triplet.Index)
                && (Type is not { } type || type == triplet.Type)
                && (From is not { } from || from == triplet.From)
                && (To is not { } to || to == triplet.To);
        }
    }

    /// <summary>What a doublet is in the encoding.</summary>
    public enum DoubletRole
    {
        /// <summary><c>Pair</c> or <c>Nothing</c>.</summary>
        Marker,
        /// <summary><c>(w: Pair f)</c>, the from-part of the pairs whose source it is.</summary>
        Wrapper,
        /// <summary><c>(p: w t)</c>, the from-part and to-part of a triplet.</summary>
        Pair,
        /// <summary><c>(d: p i)</c>, the pair of a triplet whose outer <c>(T p)</c> another triplet took.</summary>
        DuplicatePair,
        /// <summary>The outer doublet of a triplet.</summary>
        Triplet,
        /// <summary>A doublet the encoding does not know: not a triplet.</summary>
        Foreign,
    }

    public static class DoubletRoleExtensions
    {
        /// <summary>Whether the doublet is a wrapper, a pair or a duplicate pair.</summary>
        public static bool IsAuxiliary(this DoubletRole role) => role is DoubletRole.Wrapper or DoubletRole.Pair or DoubletRole.DuplicatePair;
    }

    /// <summary>
    /// The reads and writes of triplets stored in doublets (issue #151). The
    /// names end in <c>Triplet</c> because a triplet store is still a doublet
    /// store: <c>Create</c>, <c>Update</c> and <c>Delete</c> keep working on
    /// the doublets underneath.
    /// </summary>
    public interface ITriplets<TLinkAddress> : INamedTypesLinks<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>The markers of the encoding.</summary>
        TripletMarkers<TLinkAddress> Markers { get; }

        /// <summary>What the doublet <paramref name="id"/> is in the encoding; <c>null</c> when it is missing.</summary>
        DoubletRole? RoleOf(TLinkAddress id);

        /// <summary>The triplet at <paramref name="index"/>, <c>null</c> when it is not a triplet.</summary>
        Triplet<TLinkAddress>? GetTriplet(TLinkAddress index);

        /// <summary>Whether <paramref name="index"/> is a triplet.</summary>
        bool IsTriplet(TLinkAddress index);

        /// <summary>
        /// Whether a reference to <paramref name="index"/> names a triplet: the same as <see cref="IsTriplet"/>,
        /// but for a view that hides triplets, where one the subject may refer to counts as well (issue #161).
        /// </summary>
        bool ReferenceIsTriplet(TLinkAddress index) => IsTriplet(index);

        /// <summary>
        /// Creates <c>(i: type from to)</c> and returns <c>i</c>. <paramref name="type"/>
        /// must be a triplet; <paramref name="from"/> and <paramref name="to"/> triplets or the null link.
        /// </summary>
        TLinkAddress CreateTriplet(TLinkAddress type, TLinkAddress from, TLinkAddress to);

        /// <summary>Creates a triplet <c>(i: i 0 0)</c> that is its own type.</summary>
        TLinkAddress CreateTripletPoint();

        /// <summary>Makes the free address <paramref name="address"/> a triplet <c>(address: address 0 0)</c> that is its own type.</summary>
        void CreateTripletPointAt(TLinkAddress address);

        /// <summary>
        /// Makes the triplet at <paramref name="index"/> <c>(index: type from to)</c>,
        /// keeping its address, and returns the triplet as it was. The pair it no
        /// longer uses is collected when nothing else uses it.
        /// </summary>
        Triplet<TLinkAddress> UpdateTriplet(TLinkAddress index, TLinkAddress type, TLinkAddress from, TLinkAddress to);

        /// <summary>
        /// Deletes the triplet at <paramref name="index"/> and every triplet that uses
        /// a deleted one as its type, from or to; returns them all, <paramref name="index"/> first.
        /// </summary>
        IReadOnlyList<Triplet<TLinkAddress>> DeleteTriplet(TLinkAddress index);

        /// <summary>The triplets that match <paramref name="query"/>, in the order of their addresses.</summary>
        IReadOnlyList<Triplet<TLinkAddress>> EachTriplet(TripletQuery<TLinkAddress> query);

        /// <summary>The number of triplets that match <paramref name="query"/>.</summary>
        int CountTriplets(TripletQuery<TLinkAddress> query);

        /// <summary>How many other doublets use the doublet <paramref name="id"/> as their source or target.</summary>
        int DoubletUsages(TLinkAddress id);
    }

    /// <summary>What deleting a triplet takes along.</summary>
    internal static class TripletsCascade
    {
        /// <summary><paramref name="first"/> and every triplet that uses a deleted one as its type, from or to, <paramref name="first"/> first.</summary>
        public static List<Triplet<TLinkAddress>> Of<TLinkAddress>(ITriplets<TLinkAddress> triplets, Triplet<TLinkAddress> first)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            var deleted = new List<Triplet<TLinkAddress>> { first };
            var seen = new HashSet<TLinkAddress> { first.Index };
            for (var at = 0; at < deleted.Count; at++)
            {
                var index = deleted[at].Index;
                var users = triplets.EachTriplet(TripletQuery<TLinkAddress>.ByType(index))
                    .Concat(triplets.EachTriplet(TripletQuery<TLinkAddress>.ByFrom(index)))
                    .Concat(triplets.EachTriplet(TripletQuery<TLinkAddress>.ByTo(index)))
                    .Where(user => user.Index != index)
                    .Distinct()
                    .Order();
                foreach (var user in users)
                {
                    if (seen.Add(user.Index))
                    {
                        deleted.Add(user);
                    }
                }
            }
            return deleted;
        }
    }

    /// <summary>
    /// A decorator that stores triplets <c>(index: type from to)</c> in the doublets
    /// of the links it wraps (issue #151). The encoding keeps the convention of the
    /// type hierarchy of issue #148, <i>the type of a link is its source</i>:
    /// <list type="bullet">
    /// <item>a node <c>(i: T 0 0)</c> is the doublet <c>(i: T i)</c>;</item>
    /// <item><c>(i: T f t)</c> is the wrapper <c>w = (Pair f)</c>, the pair <c>p = (w t)</c>
    /// and the outer <c>(i: T p)</c>;</item>
    /// <item>when another triplet took the outer <c>(T p)</c>, a duplicate pair
    /// <c>d = (p i)</c> and the outer <c>(i: T d)</c>;</item>
    /// <item>an empty <c>f</c> or <c>t</c> is written as <c>Nothing</c>.</item>
    /// </list>
    /// The Rust counterpart is <c>link_cli::TripletsOverDoublets</c>;
    /// <c>docs/TRIPLETS.md</c> compares the encoding with the alternatives.
    /// </summary>
    /// <remarks>
    /// Reads go to the decorated links; writes go through this decorator, so a
    /// derived decorator that checks writes sees every doublet the encoding
    /// writes. The class is not sealed and every member is virtual.
    /// </remarks>
    public class TripletsOverDoublets<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, ITriplets<TLinkAddress>
        where TLinkAddress : struct,
            IUnsignedNumber<TLinkAddress>,
            IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
            IShiftOperators<TLinkAddress, int, TLinkAddress>,
            IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
            IMinMaxValue<TLinkAddress>
    {
        /// <summary>The name of the marker point of wrappers.</summary>
        public const string PairMarkerName = "Pair";

        /// <summary>The name of the marker point that stands for the empty part.</summary>
        public const string NothingMarkerName = "Nothing";

        private readonly INamedTypes<TLinkAddress> _names;

        public TripletsOverDoublets(ILinks<TLinkAddress> links, INamedTypes<TLinkAddress> names, TripletMarkers<TLinkAddress> markers) : base(links)
        {
            ArgumentNullException.ThrowIfNull(names);
            _names = names;
            Markers = markers;
        }

        /// <summary>Triplets whose markers are the points named <c>Pair</c> and <c>Nothing</c>, created when missing.</summary>
        public TripletsOverDoublets(ILinks<TLinkAddress> links, INamedTypes<TLinkAddress> names) : this(links, names, EnsureMarkers(links, names))
        {
        }

        /// <summary>Triplets whose markers are the points named <c>Pair</c> and <c>Nothing</c>, created when missing.</summary>
        public TripletsOverDoublets(INamedTypesLinks<TLinkAddress> links) : this(links, links)
        {
        }

        public TripletMarkers<TLinkAddress> Markers { get; }

        /// <summary>The points named <c>Pair</c> and <c>Nothing</c>, each created as a point and named when missing.</summary>
        public static TripletMarkers<TLinkAddress> EnsureMarkers(ILinks<TLinkAddress> links, INamedTypes<TLinkAddress> names)
        {
            return new TripletMarkers<TLinkAddress>(
                TypesDecorator<TLinkAddress>.EnsurePoint(links, names, PairMarkerName),
                TypesDecorator<TLinkAddress>.EnsurePoint(links, names, NothingMarkerName));
        }

        private static TLinkAddress Null => TLinkAddress.Zero;

        public virtual string? GetName(TLinkAddress link) => _names.GetName(link);

        public virtual TLinkAddress SetName(TLinkAddress link, string name) => _names.SetName(link, name);

        public virtual TLinkAddress GetByName(string name) => _names.GetByName(name);

        public virtual void RemoveName(TLinkAddress link) => _names.RemoveName(link);

        public virtual DoubletRole? RoleOf(TLinkAddress id)
        {
            return Stored(id) is { } link ? Classify(link).Role : null;
        }

        public virtual Triplet<TLinkAddress>? GetTriplet(TLinkAddress index)
        {
            if (Stored(index) is not { } link)
            {
                return null;
            }
            var (role, triplet) = Classify(link);
            return role == DoubletRole.Triplet ? triplet : null;
        }

        public virtual bool IsTriplet(TLinkAddress index) => GetTriplet(index) is not null;

        public virtual TLinkAddress CreateTriplet(TLinkAddress type, TLinkAddress from, TLinkAddress to)
        {
            EnsureType(type, null);
            EnsurePart(from, null, "from");
            EnsurePart(to, null, "to");
            if (from == Null && to == Null)
            {
                var node = this.Create();
                SetOuter(node, type, node);
                return node;
            }
            var pair = PairOf(from, to);
            var index = this.Create();
            if (_links.SearchOrDefault(type, pair) == Null)
            {
                SetOuter(index, type, pair);
                return index;
            }
            var duplicate = this.GetOrCreate(pair, index);
            SetOuter(index, type, duplicate);
            return index;
        }

        public virtual TLinkAddress CreateTripletPoint()
        {
            var index = this.Create();
            this.Update(index, index, index);
            return index;
        }

        public virtual void CreateTripletPointAt(TLinkAddress address)
        {
            this.EnsureCreated(address);
            this.Update(new[] { address }, new[] { address, address, address }, null);
        }

        public virtual Triplet<TLinkAddress> UpdateTriplet(TLinkAddress index, TLinkAddress type, TLinkAddress from, TLinkAddress to)
        {
            if (GetTriplet(index) is not { } before)
            {
                throw new InvalidOperationException($"Link {index} is not a triplet.");
            }
            EnsureType(type, index);
            EnsurePart(from, index, "from");
            EnsurePart(to, index, "to");
            if (before.Type == type && before.From == from && before.To == to)
            {
                return before;
            }
            var oldTarget = Stored(index)!.Value.Target;
            TLinkAddress newTarget;
            if (from == Null && to == Null)
            {
                newTarget = index;
            }
            else
            {
                var pair = PairOf(from, to);
                var taken = _links.SearchOrDefault(type, pair);
                newTarget = taken == Null || taken == index ? pair : this.GetOrCreate(pair, index);
            }
            SetOuter(index, type, newTarget);
            if (oldTarget != newTarget && oldTarget != index)
            {
                var candidates = new List<(Link<TLinkAddress>, DoubletRole)>();
                if (Stored(oldTarget) is { } old)
                {
                    candidates.Add((old, Classify(old).Role));
                }
                Collect(candidates);
            }
            return before;
        }

        public virtual IReadOnlyList<Triplet<TLinkAddress>> DeleteTriplet(TLinkAddress index)
        {
            if (GetTriplet(index) is not { } first)
            {
                throw new InvalidOperationException($"Link {index} is not a triplet.");
            }
            var deleted = TripletsCascade.Of(this, first);
            // The wrappers and pairs that touch a deleted triplet, as they are now:
            // a store that deletes what uses a deleted doublet may take some of
            // them along, and their sources must still be collected.
            var auxiliary = new List<(Link<TLinkAddress>, DoubletRole)>();
            foreach (var triplet in deleted)
            {
                var touching = Doublets(null, null, triplet.Index);
                if (Stored(triplet.Index) is { } outer && Stored(outer.Target) is { } target)
                {
                    touching.Add(target);
                }
                foreach (var link in touching)
                {
                    var role = Classify(link).Role;
                    if (role.IsAuxiliary())
                    {
                        auxiliary.Add((link, role));
                    }
                }
            }
            // Users go first, so a triplet is usually unused by the time it goes;
            // what still uses it (a cycle, a doublet of somebody else) the store
            // deletes with it, as it does with every doublet.
            for (var i = deleted.Count - 1; i >= 0; i--)
            {
                if (_links.Exists(deleted[i].Index))
                {
                    this.Delete(deleted[i].Index);
                }
            }
            Collect(auxiliary);
            return deleted;
        }

        public virtual IReadOnlyList<Triplet<TLinkAddress>> EachTriplet(TripletQuery<TLinkAddress> query)
        {
            var found = new List<Triplet<TLinkAddress>>();
            if (query.Index is { } index)
            {
                if (GetTriplet(index) is { } triplet)
                {
                    found.Add(triplet);
                }
            }
            else if (query.From is { } from && query.To is { } to && (from != Null || to != Null))
            {
                if (FindPair(from, to) is { } pair)
                {
                    found.AddRange(TripletsOnPair(pair));
                }
            }
            else if (query.From is { } fromOnly && fromOnly != Null)
            {
                var wrapper = _links.SearchOrDefault(Markers.Pair, fromOnly);
                if (wrapper != Null)
                {
                    foreach (var pair in Doublets(null, wrapper, null))
                    {
                        found.AddRange(TripletsOnPair(pair.Index));
                    }
                }
            }
            else if (query.To is { } toOnly && toOnly != Null)
            {
                foreach (var pair in Doublets(null, null, toOnly))
                {
                    if (IsWrapper(pair.Source))
                    {
                        found.AddRange(TripletsOnPair(pair.Index));
                    }
                }
            }
            else if (query.Type is { } type)
            {
                if (type != Null && !Markers.Contains(type))
                {
                    foreach (var link in Doublets(null, type, null))
                    {
                        AddTriplet(link, found);
                    }
                }
            }
            else
            {
                foreach (var link in Doublets(null, null, null))
                {
                    AddTriplet(link, found);
                }
            }
            return found.Where(query.Matches).Distinct().Order().ToList();
        }

        public virtual int CountTriplets(TripletQuery<TLinkAddress> query) => EachTriplet(query).Count;

        public virtual int DoubletUsages(TLinkAddress id)
        {
            return Doublets(null, id, null).Count(link => link.Index != id) + Doublets(null, null, id).Count(link => link.Index != id);
        }

        /// <summary>What <paramref name="link"/> is in the encoding, and the triplet when it is one.</summary>
        protected virtual (DoubletRole Role, Triplet<TLinkAddress> Triplet) Classify(Link<TLinkAddress> link)
        {
            var foreign = (DoubletRole.Foreign, default(Triplet<TLinkAddress>));
            if (Markers.Contains(link.Index))
            {
                return (DoubletRole.Marker, default);
            }
            if (link.Source == Markers.Pair)
            {
                return (DoubletRole.Wrapper, default);
            }
            if (link.Source == Null || link.Source == Markers.Nothing || Stored(link.Source) is not { } source)
            {
                return foreign;
            }
            if (source.Source == Markers.Pair)
            {
                return (DoubletRole.Pair, default);
            }
            if (IsWrapper(source.Source))
            {
                return (DoubletRole.DuplicatePair, default);
            }
            if (IsPair(source.Source))
            {
                // The source is a duplicate pair: nothing of ours has one.
                return foreign;
            }
            if (link.Target == link.Index)
            {
                return (DoubletRole.Triplet, new Triplet<TLinkAddress>(link.Index, link.Source, Null, Null));
            }
            if (Stored(link.Target) is not { } target || Markers.Contains(target.Index))
            {
                return foreign;
            }
            if (IsWrapper(target.Source) && PartsOfPair(target) is { } parts)
            {
                return (DoubletRole.Triplet, new Triplet<TLinkAddress>(link.Index, link.Source, parts.From, parts.To));
            }
            if (target.Target == link.Index && IsPair(target.Source) && Stored(target.Source) is { } pair && PartsOfPair(pair) is { } duplicateParts)
            {
                return (DoubletRole.Triplet, new Triplet<TLinkAddress>(link.Index, link.Source, duplicateParts.From, duplicateParts.To));
            }
            return foreign;
        }

        private void AddTriplet(Link<TLinkAddress> link, List<Triplet<TLinkAddress>> found)
        {
            var (role, triplet) = Classify(link);
            if (role == DoubletRole.Triplet)
            {
                found.Add(triplet);
            }
        }

        private Link<TLinkAddress>? Stored(TLinkAddress id)
        {
            if (id == Null || !_links.Exists(id))
            {
                return null;
            }
            return new Link<TLinkAddress>(_links.GetLink(id));
        }

        /// <summary>The doublets with the given parts; <c>null</c> matches any.</summary>
        private List<Link<TLinkAddress>> Doublets(TLinkAddress? index, TLinkAddress? source, TLinkAddress? target)
        {
            var any = _links.Constants.Any;
            var found = new List<Link<TLinkAddress>>();
            _links.Each(new Link<TLinkAddress>(index ?? any, source ?? any, target ?? any), link =>
            {
                found.Add(new Link<TLinkAddress>(link!));
                return _links.Constants.Continue;
            });
            return found;
        }

        private bool IsWrapper(TLinkAddress id)
        {
            return id != Null && !Markers.Contains(id) && Stored(id) is { } link && link.Source == Markers.Pair;
        }

        private bool IsPair(TLinkAddress id)
        {
            return id != Null && !Markers.Contains(id) && Stored(id) is { } link && IsWrapper(link.Source);
        }

        /// <summary>The from-part and to-part of the pair <paramref name="pair"/>.</summary>
        private (TLinkAddress From, TLinkAddress To)? PartsOfPair(Link<TLinkAddress> pair)
        {
            return Stored(pair.Source) is { } wrapper ? (Markers.Decode(wrapper.Target), Markers.Decode(pair.Target)) : null;
        }

        private void EnsureType(TLinkAddress type, TLinkAddress? itself)
        {
            if (type == Null)
            {
                throw new ArgumentException("The type of a triplet cannot be 0.");
            }
            if (!(itself is { } self && self == type) && !IsTriplet(type))
            {
                throw new ArgumentException($"The type {type} is not a triplet.");
            }
        }

        private void EnsurePart(TLinkAddress part, TLinkAddress? itself, string name)
        {
            if (part != Null && !(itself is { } self && self == part) && !IsTriplet(part))
            {
                throw new ArgumentException($"The {name} part {part} is not a triplet.");
            }
        }

        /// <summary>The pair of <paramref name="from"/> and <paramref name="to"/>, with its wrapper, created when missing.</summary>
        private TLinkAddress PairOf(TLinkAddress from, TLinkAddress to)
        {
            var wrapper = this.GetOrCreate(Markers.Pair, Markers.Encode(from));
            return this.GetOrCreate(wrapper, Markers.Encode(to));
        }

        /// <summary>The pair of <paramref name="from"/> and <paramref name="to"/>, when there is one.</summary>
        private TLinkAddress? FindPair(TLinkAddress from, TLinkAddress to)
        {
            var wrapper = _links.SearchOrDefault(Markers.Pair, Markers.Encode(from));
            if (wrapper == Null)
            {
                return null;
            }
            var pair = _links.SearchOrDefault(wrapper, Markers.Encode(to));
            return pair == Null ? null : pair;
        }

        /// <summary>The triplets whose parts are the pair: the outers <c>(T pair)</c> and the triplets of the duplicate pairs <c>(pair i)</c>.</summary>
        private List<Triplet<TLinkAddress>> TripletsOnPair(TLinkAddress pair)
        {
            var found = new List<Triplet<TLinkAddress>>();
            foreach (var outer in Doublets(null, null, pair))
            {
                AddTriplet(outer, found);
            }
            foreach (var duplicate in Doublets(null, pair, null))
            {
                if (Stored(duplicate.Target) is { } outer && outer.Target == duplicate.Index)
                {
                    AddTriplet(outer, found);
                }
            }
            return found;
        }

        /// <summary>
        /// Writes the outer doublet <c>(index: source target)</c>, refusing to merge it
        /// into another doublet that already is <c>(source target)</c>: the store would
        /// delete <paramref name="index"/> instead.
        /// </summary>
        private void SetOuter(TLinkAddress index, TLinkAddress source, TLinkAddress target)
        {
            var existing = _links.SearchOrDefault(source, target);
            if (existing != Null && existing != index)
            {
                throw new InvalidOperationException($"The doublet ({source} {target}) is already {existing}: triplet {index} would be merged into it.");
            }
            this.Update(index, source, target);
        }

        /// <summary>
        /// Deletes each wrapper, pair or duplicate pair of <paramref name="candidates"/> that
        /// nothing uses, then what it was made of, as long as nothing else uses that either.
        /// A candidate the store has already deleted leads on to what it was made of.
        /// </summary>
        private void Collect(List<(Link<TLinkAddress> Link, DoubletRole Role)> candidates)
        {
            static bool LeadsOn(DoubletRole role) => role is DoubletRole.Pair or DoubletRole.DuplicatePair;
            var pending = new Stack<TLinkAddress>();
            foreach (var (link, role) in candidates)
            {
                if (_links.Exists(link.Index))
                {
                    pending.Push(link.Index);
                }
                else if (LeadsOn(role))
                {
                    pending.Push(link.Source);
                }
            }
            while (pending.Count > 0)
            {
                var id = pending.Pop();
                if (Stored(id) is not { } link)
                {
                    continue;
                }
                var role = Classify(link).Role;
                if (!role.IsAuxiliary() || DoubletUsages(id) > 0)
                {
                    continue;
                }
                this.Delete(id);
                if (LeadsOn(role))
                {
                    pending.Push(link.Source);
                }
            }
        }
    }
}
