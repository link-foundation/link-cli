using System;
using System.Collections.Generic;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// The two points a type hierarchy needs: the root type
    /// <c>(Type: Type Type)</c> and the <c>Extends</c> marker of extra parents.
    /// <see cref="Extends"/> is the null link when the hierarchy has no
    /// multiple inheritance.
    /// </summary>
    public readonly record struct TypeMarkers<TLinkAddress>(TLinkAddress Root, TLinkAddress Extends)
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>;

    /// <summary>
    /// A type hierarchy kept in the links themselves (issue #148):
    /// <list type="bullet">
    /// <item>the type of a link is its source: <c>(v: S x)</c> is a value of type <c>S</c>;</item>
    /// <item>a type points at itself: the root <c>(Type: Type Type)</c> is a point and a
    /// sub-type of <c>T</c> is <c>(S: T S)</c>, a value of <c>T</c> whose target is itself;</item>
    /// <item>more parents (multiple inheritance) are pairs <c>(p: S T2)</c> marked by
    /// <c>(m: Extends p)</c>.</item>
    /// </list>
    /// So <c>Type -&gt; SubType -&gt; … -&gt; Value</c> is a chain of sources of any depth.
    /// </summary>
    public interface ITypes<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>The root type, <c>(Type: Type Type)</c>.</summary>
        TLinkAddress Root { get; }

        /// <summary>The <c>Extends</c> marker, or the null link when there is none.</summary>
        TLinkAddress Extends { get; }

        /// <summary>The type of a link: its source; the null link for a missing link.</summary>
        TLinkAddress TypeOf(TLinkAddress link);

        /// <summary>Whether <paramref name="link"/> is a type: it exists and points at itself.</summary>
        bool IsType(TLinkAddress link);

        /// <summary>The direct parents of a type: its source (unless it is a point) and what it extends.</summary>
        IReadOnlyList<TLinkAddress> ParentsOf(TLinkAddress type);

        /// <summary>Every type <paramref name="type"/> is, itself first, breadth first, without repeats.</summary>
        IReadOnlyList<TLinkAddress> AncestorsOf(TLinkAddress type);

        /// <summary>Whether <paramref name="link"/> is a pair that gives a type one more parent.</summary>
        bool IsExtension(TLinkAddress link);

        /// <summary>
        /// Whether <paramref name="link"/> is strictly of <paramref name="type"/>:
        /// starting from its source and the types the link itself extends, through
        /// any depth of sub-types and parents, cycles included. A type is not of
        /// itself; <see cref="IsA(TLinkAddress, TLinkAddress)"/> is.
        /// </summary>
        bool IsOf(TLinkAddress link, TLinkAddress type);

        /// <summary><see cref="IsOf(TLinkAddress, TLinkAddress)"/> for a link given as <c>(index, source, target)</c>, stored or not.</summary>
        bool IsOf(IList<TLinkAddress> link, TLinkAddress type);

        /// <summary>Whether <paramref name="link"/> is <paramref name="type"/> itself or of it.</summary>
        bool IsA(TLinkAddress link, TLinkAddress type);

        /// <summary><see cref="IsA(TLinkAddress, TLinkAddress)"/> for a link given as <c>(index, source, target)</c>, stored or not.</summary>
        bool IsA(IList<TLinkAddress> link, TLinkAddress type);

        /// <summary>Defines a type <c>(S: parent S)</c> that also extends every type in <paramref name="more"/>.</summary>
        TLinkAddress Define(TLinkAddress parent, params TLinkAddress[] more);

        /// <summary>
        /// Makes <paramref name="type"/> extend <paramref name="parent"/>: the pair
        /// <c>(p: type parent)</c> marked by <c>(m: Extends p)</c>. Returns the pair.
        /// Refused when <paramref name="parent"/> is a direct sub-type
        /// <c>(parent: type parent)</c> of <paramref name="type"/>, whose definition is
        /// that very doublet.
        /// </summary>
        TLinkAddress Extend(TLinkAddress type, TLinkAddress parent);

        /// <summary>A value <c>(v: type v)</c> of <paramref name="type"/>.</summary>
        TLinkAddress Instance(TLinkAddress type);

        /// <summary>A value <c>(v: type target)</c> of <paramref name="type"/>.</summary>
        TLinkAddress Instance(TLinkAddress type, TLinkAddress target);
    }

    /// <summary>
    /// A decorator that keeps a type hierarchy with recursive sub-typing and
    /// multiple inheritance in the links themselves. It is the C# port of
    /// <c>typeSystem</c> of <c>@link-foundation/ui-links</c>; the Rust
    /// counterpart is <c>link_cli::TypesDecorator</c>.
    /// </summary>
    /// <remarks>
    /// The pinned types of <see cref="PinnedTypesDecorator{TLinkAddress}"/> are
    /// <c>(1: 1 1)</c>, <c>(2: 1 2)</c>, <c>(3: 1 3)</c>, …: with the root at
    /// address 1 they are sub-types of it, so a pinned store is already a
    /// hierarchy. Queries read the decorated links; writes go through this
    /// decorator, so a derived decorator that checks writes sees them.
    /// The class is not sealed and every member is virtual.
    /// </remarks>
    public class TypesDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, ITypes<TLinkAddress>
        where TLinkAddress : struct,
            IUnsignedNumber<TLinkAddress>,
            IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
            IShiftOperators<TLinkAddress, int, TLinkAddress>,
            IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
            IMinMaxValue<TLinkAddress>
    {
        /// <summary>The name of the root type point.</summary>
        public const string RootName = "Type";

        /// <summary>The name of the <c>Extends</c> marker point.</summary>
        public const string ExtendsName = "Extends";

        public TypesDecorator(ILinks<TLinkAddress> links, TLinkAddress root, TLinkAddress extendsMarker) : base(links)
        {
            Root = root;
            Extends = extendsMarker;
        }

        /// <summary>A hierarchy without multiple inheritance.</summary>
        public TypesDecorator(ILinks<TLinkAddress> links, TLinkAddress root) : this(links, root, links.Constants.Null)
        {
        }

        public TypesDecorator(ILinks<TLinkAddress> links, TypeMarkers<TLinkAddress> markers) : this(links, markers.Root, markers.Extends)
        {
        }

        /// <summary>
        /// A hierarchy whose markers are the points named <c>Type</c> and
        /// <c>Extends</c>, created when missing.
        /// </summary>
        public TypesDecorator(INamedTypesLinks<TLinkAddress> links) : this(links, EnsureMarkers(links, links))
        {
        }

        public TLinkAddress Root { get; }

        public TLinkAddress Extends { get; }

        /// <summary>
        /// The points named <c>Type</c> and <c>Extends</c>, each created as a
        /// point <c>(x: x x)</c> and named when missing.
        /// </summary>
        public static TypeMarkers<TLinkAddress> EnsureMarkers(ILinks<TLinkAddress> links, INamedTypes<TLinkAddress> names)
        {
            ArgumentNullException.ThrowIfNull(links);
            ArgumentNullException.ThrowIfNull(names);
            return new TypeMarkers<TLinkAddress>(EnsurePoint(links, names, RootName), EnsurePoint(links, names, ExtendsName));
        }

        /// <summary>The point named <paramref name="name"/>, created and named when missing.</summary>
        public static TLinkAddress EnsurePoint(ILinks<TLinkAddress> links, INamedTypes<TLinkAddress> names, string name)
        {
            ArgumentNullException.ThrowIfNull(links);
            ArgumentNullException.ThrowIfNull(names);
            var known = names.GetByName(name);
            if (known != links.Constants.Null && links.Exists(known))
            {
                return known;
            }
            var id = links.Create();
            links.Update(id, id, id);
            names.SetName(id, name);
            return id;
        }

        private bool HasExtends => Extends != _links.Constants.Null;

        public virtual TLinkAddress TypeOf(TLinkAddress link)
        {
            return Exists(link) ? _links.GetSource(link) : _links.Constants.Null;
        }

        public virtual bool IsType(TLinkAddress link)
        {
            return Exists(link) && _links.GetTarget(link) == link;
        }

        public virtual IReadOnlyList<TLinkAddress> ParentsOf(TLinkAddress type)
        {
            var constants = _links.Constants;
            var parents = new List<TLinkAddress>();
            if (!Exists(type))
            {
                return parents;
            }
            var source = _links.GetSource(type);
            if (source != type && source != constants.Null)
            {
                parents.Add(source);
            }
            AddExtendedTypes(type, parents);
            return parents;
        }

        /// <summary>
        /// Adds the targets of the pairs <c>(p: type T)</c> marked by
        /// <c>(m: Extends p)</c>, in the order of the pairs' addresses.
        /// </summary>
        private void AddExtendedTypes(TLinkAddress type, List<TLinkAddress> parents)
        {
            var constants = _links.Constants;
            if (!HasExtends || type == constants.Null)
            {
                return;
            }
            var pairs = new List<(TLinkAddress Index, TLinkAddress Target)>();
            _links.Each(new Link<TLinkAddress>(constants.Any, type, constants.Any), link =>
            {
                var index = link![constants.IndexPart];
                if (index != type)
                {
                    pairs.Add((index, link[constants.TargetPart]));
                }
                return constants.Continue;
            });
            // The order of a source index is the order of its tree, not of
            // the addresses: sort, so parents come in the order they were
            // added in C# and Rust alike.
            pairs.Sort((left, right) => Comparer<TLinkAddress>.Default.Compare(left.Index, right.Index));
            foreach (var (index, target) in pairs)
            {
                if (target != constants.Null && IsExtension(index))
                {
                    parents.Add(target);
                }
            }
        }

        public virtual IReadOnlyList<TLinkAddress> AncestorsOf(TLinkAddress type)
        {
            var ancestors = new List<TLinkAddress> { type };
            var seen = new HashSet<TLinkAddress> { type };
            for (var i = 0; i < ancestors.Count; i++)
            {
                foreach (var parent in ParentsOf(ancestors[i]))
                {
                    if (seen.Add(parent))
                    {
                        ancestors.Add(parent);
                    }
                }
            }
            return ancestors;
        }

        public virtual bool IsExtension(TLinkAddress link)
        {
            var constants = _links.Constants;
            return HasExtends && link != constants.Null && _links.SearchOrDefault(Extends, link) != constants.Null;
        }

        public virtual bool IsOf(TLinkAddress link, TLinkAddress type)
        {
            return Exists(link) && IsOf(new Link<TLinkAddress>(_links.GetLink(link)), type);
        }

        public virtual bool IsOf(IList<TLinkAddress> link, TLinkAddress type)
        {
            ArgumentNullException.ThrowIfNull(link);
            var constants = _links.Constants;
            var index = link[constants.IndexPart];
            var seen = new HashSet<TLinkAddress>();
            // A link is of its source and of every type it extends itself;
            // the pairs that give a type more parents are of type Extends,
            // not values of the type they extend.
            var starts = new List<TLinkAddress>();
            if (IsExtension(index))
            {
                starts.Add(Extends);
            }
            else
            {
                starts.Add(link[constants.SourcePart]);
                if (index != constants.Null && _links.Exists(index))
                {
                    AddExtendedTypes(index, starts);
                }
            }
            var stack = new Stack<TLinkAddress>();
            for (var i = starts.Count - 1; i >= 0; i--)
            {
                stack.Push(starts[i]);
            }
            while (stack.Count > 0)
            {
                var at = stack.Pop();
                if (at == type)
                {
                    return true;
                }
                if (at == constants.Null || !seen.Add(at))
                {
                    continue;
                }
                var parents = ParentsOf(at);
                for (var i = 0; i < parents.Count; i++)
                {
                    stack.Push(parents[i]);
                }
            }
            return false;
        }

        public virtual bool IsA(TLinkAddress link, TLinkAddress type)
        {
            return Exists(link) && (link == type || IsOf(link, type));
        }

        public virtual bool IsA(IList<TLinkAddress> link, TLinkAddress type)
        {
            ArgumentNullException.ThrowIfNull(link);
            return link[_links.Constants.IndexPart] == type || IsOf(link, type);
        }

        public virtual TLinkAddress Define(TLinkAddress parent, params TLinkAddress[] more)
        {
            more ??= Array.Empty<TLinkAddress>();
            if (more.Length > 0 && !HasExtends)
            {
                throw new InvalidOperationException("Multiple parents need an Extends marker.");
            }
            EnsureExists(parent, nameof(parent));
            foreach (var other in more)
            {
                EnsureExists(other, nameof(more));
            }
            var id = this.Create();
            this.Update(id, parent, id);
            foreach (var other in more)
            {
                Extend(id, other);
            }
            return id;
        }

        /// <summary>Defines a direct sub-type of <see cref="Root"/>.</summary>
        public virtual TLinkAddress Define()
        {
            return Define(Root);
        }

        public virtual TLinkAddress Extend(TLinkAddress type, TLinkAddress parent)
        {
            if (!HasExtends)
            {
                throw new InvalidOperationException("Multiple parents need an Extends marker.");
            }
            EnsureExists(type, nameof(type));
            EnsureExists(parent, nameof(parent));
            if (_links.GetSource(parent) == type && _links.GetTarget(parent) == parent)
            {
                // (parent: type parent) is already the doublet (type parent):
                // marking it would turn the definition of parent into a pair.
                throw new InvalidOperationException($"Type {parent} is a direct sub-type of {type}: the pair ({type} {parent}) would be its definition.");
            }
            var pair = this.GetOrCreate(type, parent);
            this.GetOrCreate(Extends, pair);
            return pair;
        }

        public virtual TLinkAddress Instance(TLinkAddress type)
        {
            EnsureExists(type, nameof(type));
            var id = this.Create();
            this.Update(id, type, id);
            return id;
        }

        public virtual TLinkAddress Instance(TLinkAddress type, TLinkAddress target)
        {
            EnsureExists(type, nameof(type));
            EnsureExists(target, nameof(target));
            return this.GetOrCreate(type, target);
        }

        private bool Exists(TLinkAddress link)
        {
            return link != _links.Constants.Null && _links.Exists(link);
        }

        private void EnsureExists(TLinkAddress link, string parameterName)
        {
            if (!Exists(link))
            {
                throw new ArgumentException($"Link {link} does not exist.", parameterName);
            }
        }
    }
}
