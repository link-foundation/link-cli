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
    /// The writes made to a store, kept to undo them: every change a write
    /// reports, and the names the links had before (issue #149).
    /// </summary>
    /// <remarks>
    /// A store reports a change as <c>(before, after)</c>: <c>(null, link)</c> for a
    /// creation, <c>(link, null)</c> for a deletion. Undoing goes back through the
    /// changes: it deletes what was created, updates back what was updated and
    /// creates again, at the same address, what was deleted. Names are not
    /// changes: a deleted link loses its name before anyone above the names
    /// store hears of the deletion, so the names a write may touch are kept
    /// before it, with <see cref="KeepName"/> and <see cref="KeepNamesOfUsages"/>.
    /// </remarks>
    public sealed class LinksUndoLog<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        private readonly List<(Link<TLinkAddress>? Before, Link<TLinkAddress>? After)> _changes = new();
        private readonly List<(TLinkAddress Link, string? Name)> _names = new();
        private readonly HashSet<TLinkAddress> _named = new();

        /// <summary>Whether nothing is kept.</summary>
        public bool IsEmpty => _changes.Count == 0 && _names.Count == 0;

        /// <summary>The changes kept, in the order they were made.</summary>
        public IReadOnlyList<(Link<TLinkAddress>? Before, Link<TLinkAddress>? After)> Changes => _changes;

        /// <summary>Keeps one change a store reported.</summary>
        public void Record(IList<TLinkAddress>? before, IList<TLinkAddress>? after)
        {
            var kept = (before is null ? (Link<TLinkAddress>?)null : new Link<TLinkAddress>(before),
                        after is null ? (Link<TLinkAddress>?)null : new Link<TLinkAddress>(after));
            if (kept.Item1 is { } b && kept.Item2 is { } a && b.Equals(a))
            {
                return;
            }
            _changes.Add(kept);
        }

        /// <summary>Keeps the name <paramref name="link"/> has now, unless one of it is kept already.</summary>
        public void KeepName(INamedTypes<TLinkAddress> names, TLinkAddress link)
        {
            ArgumentNullException.ThrowIfNull(names);
            if (_named.Add(link))
            {
                _names.Add((link, names.GetName(link)));
            }
        }

        /// <summary>Keeps the name of the link called <paramref name="name"/> now, if any.</summary>
        public void KeepHolderOf(INamedTypesLinks<TLinkAddress> links, string name)
        {
            ArgumentNullException.ThrowIfNull(links);
            var holder = links.GetByName(name);
            if (holder != links.Constants.Null)
            {
                KeepName(links, holder);
            }
        }

        /// <summary>
        /// Keeps the names of <paramref name="link"/> and of every link that uses it,
        /// directly or not: the links deleting it deletes, or merging it repoints.
        /// </summary>
        public void KeepNamesOfUsages(INamedTypesLinks<TLinkAddress> links, TLinkAddress link)
        {
            ArgumentNullException.ThrowIfNull(links);
            foreach (var usage in UsagesOf(links, link))
            {
                KeepName(links, usage);
            }
        }

        /// <summary><paramref name="link"/> and every link that uses it, directly or not.</summary>
        public static List<TLinkAddress> UsagesOf(ILinks<TLinkAddress> links, TLinkAddress link)
        {
            ArgumentNullException.ThrowIfNull(links);
            var constants = links.Constants;
            var found = new List<TLinkAddress> { link };
            var seen = new HashSet<TLinkAddress> { link };
            for (var i = 0; i < found.Count; i++)
            {
                var at = found[i];
                foreach (var query in new[] { new Link<TLinkAddress>(constants.Any, at, constants.Any), new Link<TLinkAddress>(constants.Any, constants.Any, at) })
                {
                    links.Each(query, usage =>
                    {
                        var index = usage![constants.IndexPart];
                        if (seen.Add(index))
                        {
                            found.Add(index);
                        }
                        return constants.Continue;
                    });
                }
            }
            return found;
        }

        /// <summary>Forgets everything kept: the writes stay.</summary>
        public void Clear()
        {
            _changes.Clear();
            _names.Clear();
            _named.Clear();
        }

        /// <summary>
        /// Undoes every write kept, on <paramref name="links"/> (the store the writes
        /// were made on, not a view or a recorder over it), and forgets them.
        /// </summary>
        /// <returns>The addresses of the links the writes created, now deleted again.</returns>
        public IReadOnlyList<TLinkAddress> Undo(INamedTypesLinks<TLinkAddress> links)
        {
            ArgumentNullException.ThrowIfNull(links);
            var constants = links.Constants;
            var created = new List<TLinkAddress>();
            for (var i = _changes.Count - 1; i >= 0; i--)
            {
                var (before, after) = _changes[i];
                if (before is null && after is { } made)
                {
                    if (links.Exists(made.Index))
                    {
                        // Whatever used it is undone already.
                        links.Delete(new LinkAddress<TLinkAddress>(made.Index), null);
                    }
                    created.Add(made.Index);
                }
                else if (before is { } was && after is null)
                {
                    if (!links.Exists(was.Index))
                    {
                        links.EnsureCreated(was.Index);
                    }
                    if (was.Source != constants.Null || was.Target != constants.Null)
                    {
                        links.Update(new LinkAddress<TLinkAddress>(was.Index), new Link<TLinkAddress>(was.Index, was.Source, was.Target), null);
                    }
                }
                else if (before is { } old && links.Exists(old.Index))
                {
                    links.Update(new LinkAddress<TLinkAddress>(old.Index), new Link<TLinkAddress>(old.Index, old.Source, old.Target), null);
                }
            }
            // A name is the link's again; first take every name a write gave.
            foreach (var (link, name) in _names)
            {
                if (name != links.GetName(link) && (name is null || !links.Exists(link)))
                {
                    links.RemoveName(link);
                }
            }
            foreach (var (link, name) in _names)
            {
                if (name is not null && links.Exists(link) && name != links.GetName(link))
                {
                    links.SetName(link, name);
                }
            }
            Clear();
            return created;
        }
    }

    /// <summary>
    /// A store that keeps every write in a <see cref="LinksUndoLog{TLinkAddress}"/>
    /// while <see cref="Recording"/>, so a request that fails can be undone whole
    /// (issue #149). The links server puts it under every session.
    /// </summary>
    public class UndoableLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, INamedTypesLinks<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        private readonly INamedTypesLinks<TLinkAddress> _named;

        public UndoableLinksDecorator(INamedTypesLinks<TLinkAddress> links) : base(links)
        {
            _named = links;
            // The writes a write leads to are reported through it already.
            PermissionsDecorator<TLinkAddress>.KeepFacade(links);
        }

        /// <summary>The writes kept since <see cref="Begin"/>.</summary>
        public LinksUndoLog<TLinkAddress> Log { get; } = new();

        /// <summary>Whether writes are kept.</summary>
        public bool Recording { get; private set; }

        /// <summary>Starts keeping writes, forgetting the ones kept before.</summary>
        public void Begin()
        {
            Log.Clear();
            Recording = true;
        }

        /// <summary>Stops keeping writes; they stay.</summary>
        public void Commit()
        {
            Recording = false;
            Log.Clear();
        }

        /// <summary>Undoes every write kept since <see cref="Begin"/>.</summary>
        /// <returns>The addresses of the links the writes created, now deleted again.</returns>
        public IReadOnlyList<TLinkAddress> Rollback()
        {
            Recording = false;
            return Log.Undo(_named);
        }

        public override TLinkAddress Create(IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler) =>
            _links.Create(substitution, Recorded(handler));

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            if (Recording && restriction is { Count: > 0 })
            {
                KeepNamesBeforeWrite(restriction[_constants.IndexPart], substitution);
            }
            return _links.Update(restriction, substitution, Recorded(handler));
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            if (Recording && restriction is { Count: > 0 })
            {
                Log.KeepNamesOfUsages(_named, restriction[_constants.IndexPart]);
            }
            return _links.Delete(restriction, Recorded(handler));
        }

        public string? GetName(TLinkAddress link) => _named.GetName(link);

        public TLinkAddress SetName(TLinkAddress link, string name)
        {
            if (Recording)
            {
                Log.KeepName(_named, link);
                Log.KeepHolderOf(_named, name);
            }
            return _named.SetName(link, name);
        }

        public TLinkAddress GetByName(string name) => _named.GetByName(name);

        public void RemoveName(TLinkAddress link)
        {
            if (Recording)
            {
                Log.KeepName(_named, link);
            }
            _named.RemoveName(link);
        }

        private void KeepNamesBeforeWrite(TLinkAddress index, IList<TLinkAddress>? substitution)
        {
            KeepNamesBeforeUpdate(Log, _named, index, substitution);
        }

        /// <summary>
        /// Keeps the names an update may take: of the link, and of its usages
        /// too when it merges into an equal link and so is deleted.
        /// </summary>
        internal static void KeepNamesBeforeUpdate(LinksUndoLog<TLinkAddress> log, INamedTypesLinks<TLinkAddress> links, TLinkAddress index, IList<TLinkAddress>? substitution)
        {
            var constants = links.Constants;
            if (substitution is { Count: >= 2 })
            {
                var source = substitution[substitution.Count == 2 ? 0 : constants.SourcePart];
                var target = substitution[substitution.Count == 2 ? 1 : constants.TargetPart];
                var equal = links.SearchOrDefault(source, target);
                if (equal != constants.Null && equal != index)
                {
                    log.KeepNamesOfUsages(links, index);
                    return;
                }
            }
            log.KeepName(links, index);
        }

        private WriteHandler<TLinkAddress>? Recorded(WriteHandler<TLinkAddress>? handler)
        {
            if (!Recording)
            {
                return handler;
            }
            var @continue = _constants.Continue;
            return (before, after) =>
            {
                Log.Record(before, after);
                return handler is null ? @continue : handler(before, after);
            };
        }
    }
}
