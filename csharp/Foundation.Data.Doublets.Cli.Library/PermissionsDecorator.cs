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
    /// The store as one subject sees it (issue #149), the C# port of
    /// <c>withPermissions</c> of <c>@link-foundation/ui-links</c>; the Rust
    /// counterpart is <c>link_cli::PermissionsDecorator</c>.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <see cref="Each"/> and <see cref="Count"/> skip the links the subject may
    /// not read, so a hidden link is as missing as a deleted one: to
    /// <c>Exists</c>, to <c>GetLink</c>, and to every write, which answers
    /// <c>Link not found</c>. A write the subject may not make throws
    /// <see cref="ForbiddenException"/> and changes nothing. An update needs the
    /// right to write the link before and after it.
    /// </para>
    /// <para>
    /// A link the subject created through this view is its own while it has no
    /// type (its source is 0), so it can give it one, and undoing the update
    /// that typed it makes it its own again. <see cref="Forget"/> drops that
    /// claim when the link is gone, so a later link at the same address is not
    /// the subject's.
    /// </para>
    /// <para>
    /// The store below may change more than the link asked for: deleting a link
    /// deletes the links that use it, and an update that makes a link equal to
    /// another merges them, repointing the usages. Every link a write changes
    /// must be the subject's to change; when one is not, the write is undone
    /// and refused as a whole.
    /// </para>
    /// <para>
    /// Names are references, not links: <see cref="GetName"/> and
    /// <see cref="GetByName"/> answer for every link, so a subject may refer to
    /// a type it cannot read, as it does when it types its own link. Naming or
    /// unnaming a link needs the right to write it.
    /// </para>
    /// </remarks>
    public class PermissionsDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, INamedTypesLinks<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        private readonly INamedTypesLinks<TLinkAddress> _named;
        private readonly Func<TLinkAddress> _subject;
        private readonly HashSet<TLinkAddress> _created = new();

        /// <param name="links">The store below.</param>
        /// <param name="policy">Who may read and write what.</param>
        /// <param name="subject">The subject, asked at every call (the null link is a guest).</param>
        public PermissionsDecorator(INamedTypesLinks<TLinkAddress> links, IPermissionPolicy<TLinkAddress> policy, Func<TLinkAddress> subject) : base(links)
        {
            ArgumentNullException.ThrowIfNull(policy);
            ArgumentNullException.ThrowIfNull(subject);
            _named = links;
            Policy = policy;
            _subject = subject;
            LinksExtensions.KeepOwnFacade(links);
        }

        /// <param name="links">The store below.</param>
        /// <param name="policy">Who may read and write what.</param>
        /// <param name="subject">The subject (the null link is a guest).</param>
        public PermissionsDecorator(INamedTypesLinks<TLinkAddress> links, IPermissionPolicy<TLinkAddress> policy, TLinkAddress subject)
            : this(links, policy, () => subject)
        {
        }

        /// <summary>Who may read and write what.</summary>
        public IPermissionPolicy<TLinkAddress> Policy { get; }

        /// <summary>The subject the store is seen as.</summary>
        public TLinkAddress Subject => _subject();

        /// <summary>Whether <paramref name="link"/> is the subject's own: created through this view and still without a type.</summary>
        public virtual bool Owns(IList<TLinkAddress> link)
        {
            ArgumentNullException.ThrowIfNull(link);
            return link[_constants.SourcePart] == _constants.Null && _created.Contains(link[_constants.IndexPart]);
        }

        /// <summary>Whether the subject may read <paramref name="link"/>.</summary>
        public virtual bool CanRead(IList<TLinkAddress> link) => Owns(link) || Policy.CanRead(Subject, link);

        /// <summary>Whether the subject may write <paramref name="link"/> as it is.</summary>
        public virtual bool CanWrite(IList<TLinkAddress> link) => Owns(link) || Policy.CanWrite(Subject, link);

        /// <summary>The link at <paramref name="index"/> is gone: a new one there is not the subject's own.</summary>
        public virtual void Forget(TLinkAddress index) => _created.Remove(index);

        /// <summary>The link at <paramref name="index"/> is one the subject made on the store below.</summary>
        protected void Remember(TLinkAddress index) => _created.Add(index);

        /// <summary>Whether the link at <paramref name="index"/> is one the subject made through this view.</summary>
        protected bool Remembers(TLinkAddress index) => _created.Contains(index);

        public override TLinkAddress Count(IList<TLinkAddress>? restriction)
        {
            var count = TLinkAddress.Zero;
            _links.Each(restriction, link =>
            {
                if (CanRead(link!))
                {
                    count++;
                }
                return _constants.Continue;
            });
            return count;
        }

        public override TLinkAddress Each(IList<TLinkAddress>? restriction, ReadHandler<TLinkAddress>? handler) =>
            _links.Each(restriction, link => CanRead(link!) ? (handler is null ? _constants.Continue : handler(link)) : _constants.Continue);

        public override TLinkAddress Create(IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            if (!Policy.CanCreate(Subject))
            {
                throw new ForbiddenException("create");
            }
            var created = TLinkAddress.Zero;
            var result = _links.Create(substitution, (before, after) =>
            {
                if (before is null && after is not null)
                {
                    created = after[_constants.IndexPart];
                    _created.Add(created);
                }
                return handler is null ? _constants.Continue : handler(before, after);
            });
            if (created == TLinkAddress.Zero && result != _constants.Null && result != _constants.Break && result != _constants.Continue)
            {
                _created.Add(result);
            }
            return result;
        }

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            var index = IndexOf(restriction, "update");
            var before = Visible(index);
            if (before is null)
            {
                return _links.Update(restriction, substitution, handler);
            }
            var after = After(index, substitution, before.Value);
            if (!(Owns(before.Value) && after.Source == _constants.Null)
                && (!CanWrite(before.Value) || !Policy.CanWrite(Subject, after)))
            {
                throw new ForbiddenException($"update {index}");
            }
            var log = new LinksUndoLog<TLinkAddress>();
            UndoableLinksDecorator<TLinkAddress>.KeepNamesBeforeUpdate(log, _named, index, substitution);
            return Checked($"update {index}", index, log, recorder => _links.Update(restriction, substitution, recorder), handler);
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            var index = IndexOf(restriction, "delete");
            var before = Visible(index);
            if (before is null)
            {
                return _links.Delete(restriction, handler);
            }
            if (!CanWrite(before.Value))
            {
                throw new ForbiddenException($"delete {index}");
            }
            var log = new LinksUndoLog<TLinkAddress>();
            log.KeepNamesOfUsages(_named, index);
            return Checked($"delete {index}", index, log, recorder => _links.Delete(restriction, recorder), handler);
        }

        /// <summary>The name of any link: names are references.</summary>
        public virtual string? GetName(TLinkAddress link) => _named.GetName(link);

        /// <summary>The link of any name: names are references.</summary>
        public virtual TLinkAddress GetByName(string name) => _named.GetByName(name);

        public virtual TLinkAddress SetName(TLinkAddress link, string name)
        {
            RequireWritable(link, "set-name");
            var holder = _named.GetByName(name);
            if (holder != _constants.Null && holder != link && _links.Exists(holder) && !CanWrite(new Link<TLinkAddress>(_links.GetLink(holder))))
            {
                // Taking the name takes it from its link.
                throw new ForbiddenException($"set-name {link}");
            }
            return _named.SetName(link, name);
        }

        public virtual void RemoveName(TLinkAddress link)
        {
            if (_links.Exists(link) && !CanRead(new Link<TLinkAddress>(_links.GetLink(link))))
            {
                // Nothing the subject sees has a name to remove.
                return;
            }
            if (_links.Exists(link))
            {
                RequireWritable(link, "remove-name");
            }
            _named.RemoveName(link);
        }

        private void RequireWritable(TLinkAddress index, string what)
        {
            var link = Visible(index) ?? throw new ArgumentException($"Link not found: {index}");
            if (!CanWrite(link))
            {
                throw new ForbiddenException($"{what} {index}");
            }
        }

        /// <summary>The stored link at <paramref name="index"/>, null when there is none; a hidden one is not found.</summary>
        private Link<TLinkAddress>? Visible(TLinkAddress index)
        {
            if (index == _constants.Null || !_links.Exists(index))
            {
                return null;
            }
            var link = new Link<TLinkAddress>(_links.GetLink(index));
            if (!CanRead(link))
            {
                throw new ArgumentException($"Link not found: {index}");
            }
            return link;
        }

        private TLinkAddress IndexOf(IList<TLinkAddress>? restriction, string what)
        {
            var index = restriction is { Count: > 0 } ? restriction[_constants.IndexPart] : _constants.Null;
            if (index == _constants.Any)
            {
                // A write to many links at once would pass by the checks of each.
                throw new ForbiddenException($"{what} *");
            }
            return index;
        }

        private Link<TLinkAddress> After(TLinkAddress index, IList<TLinkAddress>? substitution, Link<TLinkAddress> before)
        {
            if (substitution is null || substitution.Count == 0)
            {
                return before;
            }
            if (substitution.Count == 2)
            {
                return new Link<TLinkAddress>(index, substitution[0], substitution[1]);
            }
            return new Link<TLinkAddress>(index, substitution[_constants.SourcePart], substitution[_constants.TargetPart]);
        }

        /// <summary>
        /// Makes a write on the store below, then checks every link it changed:
        /// the link asked for is checked already, any other must be the
        /// subject's to change, or the write is undone and refused.
        /// </summary>
        private TLinkAddress Checked(string what, TLinkAddress index, LinksUndoLog<TLinkAddress> log, Func<WriteHandler<TLinkAddress>, TLinkAddress> write, WriteHandler<TLinkAddress>? handler)
        {
            var result = write((before, after) =>
            {
                log.Record(before, after);
                return _constants.Continue;
            });
            var net = new Dictionary<TLinkAddress, (Link<TLinkAddress>? First, Link<TLinkAddress>? Last)>();
            var order = new List<TLinkAddress>();
            foreach (var (before, after) in log.Changes)
            {
                var at = (before ?? after)!.Value.Index;
                if (net.TryGetValue(at, out var known))
                {
                    net[at] = (known.First, after);
                }
                else
                {
                    net[at] = (before, after);
                    order.Add(at);
                }
            }
            foreach (var at in order)
            {
                var (first, last) = net[at];
                if (at == index || Allowed(first, last))
                {
                    continue;
                }
                var undone = log.Undo(_named);
                foreach (var gone in undone)
                {
                    _created.Remove(gone);
                }
                throw new ForbiddenException(what);
            }
            foreach (var (at, (first, last)) in net)
            {
                if (last is null)
                {
                    _created.Remove(at);
                }
            }
            if (handler is not null)
            {
                foreach (var (before, after) in log.Changes)
                {
                    if (handler(before, after) == _constants.Break)
                    {
                        break;
                    }
                }
            }
            return result;
        }

        /// <summary>Whether the subject may change a link from <paramref name="before"/> to <paramref name="after"/>.</summary>
        private bool Allowed(Link<TLinkAddress>? before, Link<TLinkAddress>? after)
        {
            if (before is { } was && after is { } now && was.Equals(now))
            {
                return true;
            }
            if (before is null)
            {
                return Policy.CanCreate(Subject) && (after is not { } made || made.Source == _constants.Null || Policy.CanWrite(Subject, made));
            }
            if (after is null)
            {
                return CanWrite(before.Value);
            }
            if (Owns(before.Value) && after.Value.Source == _constants.Null)
            {
                return true;
            }
            return CanWrite(before.Value) && Policy.CanWrite(Subject, after.Value);
        }
    }
}
