using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>A write a subject may not make (issue #149); the message is <c>Forbidden: what</c>.</summary>
    public class ForbiddenException : InvalidOperationException
    {
        public ForbiddenException(string what) : base($"Forbidden: {what}")
        {
            What = what;
        }

        public ForbiddenException(string what, Exception innerException) : base($"Forbidden: {what}", innerException)
        {
            What = what;
        }

        /// <summary>What was refused, such as <c>update 7</c>.</summary>
        public string What { get; }
    }

    /// <summary>What a rule allows.</summary>
    public enum PermissionAccess
    {
        /// <summary>Reading: <c>Each</c> and <c>Count</c> show the link.</summary>
        Read,
        /// <summary>Reading and writing: the link may be created, updated, deleted and named.</summary>
        Write,
        /// <summary>Referring: the link may be used in another link, not seen.</summary>
        Reference,
    }

    /// <summary>
    /// The four points of the data permission rules are made of: the markers
    /// <c>AllowRead</c>, <c>AllowWrite</c> and <c>AllowReference</c> of rules and
    /// <c>Anything</c>, which matches every subject or every link.
    /// </summary>
    public readonly record struct PermissionMarkers<TLinkAddress>(TLinkAddress AllowRead, TLinkAddress AllowWrite, TLinkAddress Anything, TLinkAddress AllowReference)
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>The name of the marker of read rules.</summary>
        public const string AllowReadName = "AllowRead";

        /// <summary>The name of the marker of write rules.</summary>
        public const string AllowWriteName = "AllowWrite";

        /// <summary>The name of the marker that matches every subject or link.</summary>
        public const string AnythingName = "Anything";

        /// <summary>The name of the marker of reference rules.</summary>
        public const string AllowReferenceName = "AllowReference";

        /// <summary>The marker of the rules of <paramref name="access"/>.</summary>
        public TLinkAddress Of(PermissionAccess access) => access switch
        {
            PermissionAccess.Write => AllowWrite,
            PermissionAccess.Reference => AllowReference,
            _ => AllowRead,
        };
    }

    /// <summary>Who may read, write and refer to what.</summary>
    /// <remarks>
    /// A subject is a link address; the null link (0) is a guest, which no rule
    /// but one for <c>Anything</c> matches. A link is given as
    /// <c>(index, source, target)</c>, so a link that is not stored yet, or not
    /// any more, can be asked about too.
    /// </remarks>
    public interface IPermissionPolicy<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        /// <summary>The markers the rules are made of.</summary>
        PermissionMarkers<TLinkAddress> Markers { get; }

        /// <summary>Whether <paramref name="subject"/> may read <paramref name="link"/>; writing allows reading.</summary>
        bool CanRead(TLinkAddress subject, IList<TLinkAddress> link);

        /// <summary>Whether <paramref name="subject"/> may write <paramref name="link"/>, as it is or as it will be.</summary>
        bool CanWrite(TLinkAddress subject, IList<TLinkAddress> link);

        /// <summary>Whether <paramref name="subject"/> may write anything at all, and so create links.</summary>
        bool CanCreate(TLinkAddress subject);

        /// <summary>
        /// Whether <paramref name="subject"/> may refer to <paramref name="link"/> although it need not
        /// read it. A missing address is asked about as an untyped link. A policy without this right
        /// allows references to readable links only.
        /// </summary>
        bool CanReference(TLinkAddress subject, IList<TLinkAddress> link) => false;
    }

    /// <summary>
    /// Permissions stored as links (issue #149), the C# port of <c>createPolicy</c>
    /// of <c>@link-foundation/ui-links</c>; the Rust counterpart is
    /// <c>link_cli::PermissionPolicy</c>.
    /// </summary>
    /// <remarks>
    /// <para>
    /// A rule is a pair <c>(g: Subject Object)</c> marked as <c>(r: AllowRead g)</c>
    /// or <c>(r: AllowWrite g)</c>: the subjects that are <c>Subject</c> (the subject
    /// itself, or a link of that type) may read, or read and write, the links of
    /// type <c>Object</c>: its values and sub-types, not the link of <c>Object</c>
    /// itself, which is changed by who may change its parent. <c>Anything</c> in
    /// either place matches every subject or every link. Types are those of
    /// <see cref="ITypes{TLinkAddress}"/> (issue #148): through any depth of
    /// sub-types and multiple inheritance; the pairs that wire the hierarchy are
    /// of type <c>Extends</c>.
    /// </para>
    /// <para>
    /// The markers are points of the data, so every address a rule holds is an
    /// address of the data. The rules (the pairs and their marks) live in the data
    /// itself or in a store of their own. Rules in the data are links like any
    /// other: a subject that may write them may change the rules, so give
    /// <c>Anything</c> writes to administrators only, or keep the rules apart.
    /// </para>
    /// <para>
    /// Referring to a link is a right of its own (issue #161). Without it an address the
    /// subject may not read answers <c>Link not found</c>, exactly as a missing one. A subject
    /// may refer to an address when it may read the link there, when it is that link, when a
    /// rule <c>(r: AllowReference (g: Subject Object))</c> covers it (<c>Object</c> itself and
    /// the links of type <c>Object</c>), or when it is the <c>Object</c> of an <c>AllowRead</c>
    /// or <c>AllowWrite</c> rule of the subject, so <c>User writes Note</c> is enough to type a
    /// link as <c>Note</c>. <c>AllowReference</c> does not allow reading. A rule from
    /// <c>Anything</c> to <c>Anything</c> lets every subject refer to every link.
    /// </para>
    /// </remarks>
    public class PermissionPolicy<TLinkAddress> : IPermissionPolicy<TLinkAddress>
        where TLinkAddress : struct,
            IUnsignedNumber<TLinkAddress>,
            IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
            IShiftOperators<TLinkAddress, int, TLinkAddress>,
            IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
            IMinMaxValue<TLinkAddress>
    {
        private readonly ILinks<TLinkAddress> _rules;
        private readonly ITypes<TLinkAddress> _types;

        /// <param name="rules">The store of the rules: the data itself or a store of its own.</param>
        /// <param name="markers">The markers, points of the data.</param>
        /// <param name="types">The types of the data.</param>
        public PermissionPolicy(ILinks<TLinkAddress> rules, PermissionMarkers<TLinkAddress> markers, ITypes<TLinkAddress> types)
        {
            ArgumentNullException.ThrowIfNull(rules);
            ArgumentNullException.ThrowIfNull(types);
            _rules = rules;
            _types = types;
            Markers = markers;
        }

        /// <summary>
        /// The policy of the rules in <paramref name="rules"/> over <paramref name="data"/>,
        /// whose points named <c>Type</c>, <c>Extends</c>, <c>AllowRead</c>,
        /// <c>AllowWrite</c>, <c>Anything</c> and <c>AllowReference</c> are created when missing.
        /// </summary>
        public PermissionPolicy(INamedTypesLinks<TLinkAddress> data, ILinks<TLinkAddress> rules)
            : this(rules, EnsureMarkers(data, data), new TypesDecorator<TLinkAddress>(data))
        {
        }

        /// <summary>
        /// The points named <c>AllowRead</c>, <c>AllowWrite</c>, <c>Anything</c> and
        /// <c>AllowReference</c> in the data, each created as a point <c>(x: x x)</c> and named
        /// when missing. <c>AllowReference</c> comes last, so the others keep their addresses in
        /// a store made before it was added.
        /// </summary>
        public static PermissionMarkers<TLinkAddress> EnsureMarkers(ILinks<TLinkAddress> links, INamedTypes<TLinkAddress> names)
        {
            ArgumentNullException.ThrowIfNull(links);
            ArgumentNullException.ThrowIfNull(names);
            return new PermissionMarkers<TLinkAddress>(
                TypesDecorator<TLinkAddress>.EnsurePoint(links, names, PermissionMarkers<TLinkAddress>.AllowReadName),
                TypesDecorator<TLinkAddress>.EnsurePoint(links, names, PermissionMarkers<TLinkAddress>.AllowWriteName),
                TypesDecorator<TLinkAddress>.EnsurePoint(links, names, PermissionMarkers<TLinkAddress>.AnythingName),
                TypesDecorator<TLinkAddress>.EnsurePoint(links, names, PermissionMarkers<TLinkAddress>.AllowReferenceName));
        }

        public PermissionMarkers<TLinkAddress> Markers { get; }

        /// <summary>The store of the rules.</summary>
        public ILinks<TLinkAddress> Rules => _rules;

        /// <summary>The types of the data.</summary>
        public ITypes<TLinkAddress> Types => _types;

        /// <summary>The rules <c>(Subject Object)</c> that <paramref name="marker"/> marks, in the order of their marks.</summary>
        public virtual IReadOnlyList<(TLinkAddress Subject, TLinkAddress Object)> Granted(TLinkAddress marker)
        {
            var constants = _rules.Constants;
            var marks = new List<(TLinkAddress Mark, TLinkAddress Rule)>();
            _rules.Each(new Link<TLinkAddress>(constants.Any, marker, constants.Any), mark =>
            {
                var index = mark![constants.IndexPart];
                var rule = mark[constants.TargetPart];
                // The marker point (x: x x) itself is no mark; in a store of
                // rules of their own a mark may sit at the marker's address.
                if (index != marker || rule != marker)
                {
                    marks.Add((index, rule));
                }
                return constants.Continue;
            });
            marks.Sort((left, right) => Comparer<TLinkAddress>.Default.Compare(left.Mark, right.Mark));
            var granted = new List<(TLinkAddress, TLinkAddress)>();
            foreach (var (_, rule) in marks)
            {
                if (rule != constants.Null && _rules.Exists(rule))
                {
                    granted.Add((_rules.GetSource(rule), _rules.GetTarget(rule)));
                }
            }
            return granted;
        }

        public virtual bool CanRead(TLinkAddress subject, IList<TLinkAddress> link) =>
            Allows(Markers.AllowRead, subject, link) || Allows(Markers.AllowWrite, subject, link);

        public virtual bool CanWrite(TLinkAddress subject, IList<TLinkAddress> link) =>
            Allows(Markers.AllowWrite, subject, link);

        public virtual bool CanCreate(TLinkAddress subject) =>
            Granted(Markers.AllowWrite).Any(rule => IsSubject(subject, rule.Subject));

        public virtual bool CanReference(TLinkAddress subject, IList<TLinkAddress> link)
        {
            ArgumentNullException.ThrowIfNull(link);
            var index = link[_rules.Constants.IndexPart];
            if ((subject != _rules.Constants.Null && index == subject) || Allows(Markers.AllowReference, subject, link))
            {
                return true;
            }
            // The object of a rule of the subject is named by that rule: a readable
            // link of that type shows its address anyway.
            return new[] { Markers.AllowRead, Markers.AllowWrite, Markers.AllowReference }
                .Any(marker => Granted(marker).Any(rule => rule.Object == index && IsSubject(subject, rule.Subject)));
        }

        /// <summary>Whether a rule of <paramref name="marker"/> lets <paramref name="subject"/> at <paramref name="link"/>.</summary>
        protected virtual bool Allows(TLinkAddress marker, TLinkAddress subject, IList<TLinkAddress> link)
        {
            ArgumentNullException.ThrowIfNull(link);
            return Granted(marker).Any(rule => IsSubject(subject, rule.Subject) && IsObject(link, rule.Object));
        }

        /// <summary>Whether <paramref name="subject"/> is <paramref name="type"/> or of it; a guest is only <c>Anything</c>.</summary>
        protected virtual bool IsSubject(TLinkAddress subject, TLinkAddress type) =>
            type == Markers.Anything || (subject != _rules.Constants.Null && _types.IsA(subject, type));

        /// <summary>Whether <paramref name="link"/> is of <paramref name="type"/>.</summary>
        protected virtual bool IsObject(IList<TLinkAddress> link, TLinkAddress type) =>
            type == Markers.Anything || _types.IsOf(link, type);

        /// <summary>
        /// Writes a rule into <paramref name="rules"/>: <paramref name="subject"/> may
        /// <paramref name="access"/> the links of <paramref name="object"/>. The rule is
        /// the pair <c>(subject object)</c> marked by <c>(marker rule)</c>; returns the pair.
        /// </summary>
        public static TLinkAddress Allow(
            ILinks<TLinkAddress> rules,
            PermissionMarkers<TLinkAddress> markers,
            PermissionAccess access,
            TLinkAddress subject,
            TLinkAddress @object)
        {
            ArgumentNullException.ThrowIfNull(rules);
            if (access is not (PermissionAccess.Read or PermissionAccess.Write or PermissionAccess.Reference))
            {
                throw new ArgumentOutOfRangeException(nameof(access), access, $"Unknown access: {access}");
            }
            if (subject == markers.AllowRead || subject == markers.AllowWrite || subject == markers.AllowReference)
            {
                // A pair from a marker would read as a mark.
                throw new ArgumentException("A marker cannot be a subject", nameof(subject));
            }
            // A rule given twice is the same rule: the pair and its mark are
            // found before they are made, as a store of unique doublets would
            // merge them anyway.
            var rule = SearchOrCreate(rules, subject, @object);
            SearchOrCreate(rules, markers.Of(access), rule);
            return rule;
        }

        /// <summary>Parses <c>read</c> or <c>write</c>.</summary>
        private static TLinkAddress SearchOrCreate(ILinks<TLinkAddress> links, TLinkAddress source, TLinkAddress target)
        {
            var found = links.SearchOrDefault(source, target);
            if (found != links.Constants.Null)
            {
                return found;
            }
            var link = links.Create();
            links.Update(link, source, target);
            return link;
        }

        public static PermissionAccess ParseAccess(string access) => access switch
        {
            "read" => PermissionAccess.Read,
            "write" => PermissionAccess.Write,
            "reference" => PermissionAccess.Reference,
            _ => throw new ArgumentException($"Unknown access: {access}", nameof(access)),
        };
    }
}
