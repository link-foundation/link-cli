using System;
using System.Collections.Generic;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>What a <see cref="CacheLinksDecorator{TLinkAddress}"/> enforces. Each is optional, and the default is neither.</summary>
    /// <param name="MaxLinks">How many links the store may hold, reached by evicting.</param>
    /// <param name="TimeToLive">The milliseconds a link created here has before it comes due.</param>
    public sealed record CacheOptions(int? MaxLinks = null, ulong? TimeToLive = null);

    /// <summary>
    /// A links store used as a cache: links can expire, and the store can be kept to
    /// a number of links by evicting the ones used least recently. It has the API of
    /// any other store, so only a store that selects it pays for it. See
    /// <c>docs/ENGINE-CACHE.md</c>. The Rust twin is <c>link_cli::CacheLinks</c>, the
    /// JavaScript one <c>CacheLinks</c> of <c>@link-foundation/link-cli-client</c>.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The two rules differ on purpose. A deadline is a promise, so a link that comes
    /// due is deleted with whatever the store's own delete takes with it. Capacity
    /// only has to free a place, so it evicts a link nothing else uses and takes
    /// nothing else along. When every link is in use the write is refused with
    /// <see cref="InvalidOperationException"/>.
    /// </para>
    /// <para>
    /// There is no worker. Everything due is deleted by the next call that touches
    /// the store, or by <see cref="PurgeExpired"/> when a caller wants to run it on a
    /// timer. Deadlines live in memory: a store opened again has links without one.
    /// </para>
    /// <para>
    /// A link is made here in two steps, <c>Create</c> and then <c>Update</c>, and only
    /// the second one says what it refers to. So capacity is kept when the content of
    /// the new link arrives, or by the next write when none does, and the source and
    /// target it names are not evicted for it. Until then the new link is counted
    /// over the limit. A refused link is deleted again.
    /// </para>
    /// </remarks>
    public class CacheLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, INamedTypesLinks<TLinkAddress>, IComposedLinks<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
    {
        private sealed class Entry
        {
            public ulong Used;
            public (ulong Deadline, ulong Stamp)? Due;
        }

        private readonly INamedTypesLinks<TLinkAddress> _named;
        private readonly int? _maxLinks;
        private readonly ulong? _timeToLive;
        private readonly Func<ulong> _clock;
        private readonly Dictionary<TLinkAddress, Entry> _entries = new();
        private readonly Dictionary<TLinkAddress, uint> _usages = new();
        private readonly SortedDictionary<ulong, TLinkAddress> _evictable = new();
        private readonly SortedDictionary<(ulong Deadline, ulong Stamp), TLinkAddress> _deadlines = new();
        private ulong _now;
        private ulong _uses;
        private TLinkAddress? _unsettled;

        /// <param name="links">The store below. Every write has to go through this decorator for it to know which links exist.</param>
        /// <param name="options">What to enforce.</param>
        /// <param name="clock">Milliseconds, the system clock by default. A step back delays what comes due.</param>
        public CacheLinksDecorator(INamedTypesLinks<TLinkAddress> links, CacheOptions? options = null, Func<ulong>? clock = null) : base(links)
        {
            _named = links;
            _maxLinks = options?.MaxLinks;
            _clock = clock ?? (() => (ulong)DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
            LinksExtensions.KeepOwnFacade(links);
            // The links already there are the least recently used, in the order of
            // their addresses, and have no deadline.
            _links.Each(null, link =>
            {
                Changed(null, link);
                return _constants.Continue;
            });
            _timeToLive = options?.TimeToLive;
        }

        /// <summary>Deletes what is due now; returns how many links that deleted, cascades included.</summary>
        public int PurgeExpired()
        {
            _now = _clock();
            var deleted = 0;
            while (_deadlines.Count > 0)
            {
                using var soonest = _deadlines.GetEnumerator();
                soonest.MoveNext();
                if (soonest.Current.Key.Deadline > _now)
                {
                    break;
                }
                _links.Delete(new LinkAddress<TLinkAddress>(soonest.Current.Value), (before, after) =>
                {
                    deleted += IsNone(after) ? 1 : 0;
                    Changed(before, after);
                    return _constants.Continue;
                });
            }
            return deleted;
        }

        /// <summary>Gives <paramref name="link"/> a deadline in place of any it had; false when there is no such link. A time of zero deletes it now, as Redis <c>EXPIRE</c> does.</summary>
        public bool Expire(TLinkAddress link, ulong milliseconds)
        {
            PurgeExpired();
            if (!_entries.ContainsKey(link))
            {
                return false;
            }
            var remaining = ulong.MaxValue - _now;
            SetDeadline(link, _now + Math.Min(milliseconds, remaining));
            PurgeExpired();
            return true;
        }

        /// <summary>Takes the deadline of <paramref name="link"/> away; false when it had none.</summary>
        public bool Persist(TLinkAddress link)
        {
            PurgeExpired();
            if (!_entries.TryGetValue(link, out var entry) || entry.Due is null)
            {
                return false;
            }
            SetDeadline(link, null);
            return true;
        }

        /// <summary>The milliseconds <paramref name="link"/> has left; null when it has no deadline or is gone.</summary>
        public ulong? TimeToLive(TLinkAddress link)
        {
            PurgeExpired();
            return _entries.TryGetValue(link, out var entry) && entry.Due is { } due ? due.Deadline - _now : null;
        }

        public override TLinkAddress Count(IList<TLinkAddress>? restriction)
        {
            PurgeExpired();
            return _links.Count(restriction);
        }

        /// <summary>A read of one link by its address, or by its source and target, uses what it finds; a scan does not.</summary>
        public override TLinkAddress Each(IList<TLinkAddress>? restriction, ReadHandler<TLinkAddress>? handler)
        {
            PurgeExpired();
            if (!NamesOneLink(restriction))
            {
                return _links.Each(restriction, handler);
            }
            return _links.Each(restriction, link =>
            {
                Touch(link![_constants.IndexPart]);
                return handler is null ? _constants.Continue : handler(link);
            });
        }

        public override TLinkAddress Create(IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            PurgeExpired();
            Settle();
            if (_maxLinks == 0)
            {
                throw Full();
            }
            var observed = Observed(handler);
            // What a write returns is the flow of the handler, not the new address.
            return _links.Create(substitution, (before, after) =>
            {
                _unsettled = after![_constants.IndexPart];
                return observed(before, after);
            });
        }

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            PurgeExpired();
            if (substitution is { Count: >= 3 } content && restriction is { Count: > 0 } && _unsettled is { } unsettled && unsettled == restriction[_constants.IndexPart])
            {
                SettleKeeping(content[_constants.SourcePart], content[_constants.TargetPart]);
            }
            else
            {
                Settle();
            }
            return _links.Update(restriction, substitution, Observed(handler));
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            PurgeExpired();
            Settle();
            return _links.Delete(restriction, Observed(handler));
        }

        public virtual string? GetName(TLinkAddress link)
        {
            PurgeExpired();
            return _named.GetName(link);
        }

        public virtual TLinkAddress GetByName(string name)
        {
            PurgeExpired();
            return _named.GetByName(name);
        }

        public virtual TLinkAddress SetName(TLinkAddress link, string name)
        {
            PurgeExpired();
            return _named.SetName(link, name);
        }

        public virtual void RemoveName(TLinkAddress link)
        {
            PurgeExpired();
            _named.RemoveName(link);
        }

        public bool ReassignsNames => _named.ReassignsNames;

        public QueryLimits? QueryLimits => _named.QueryLimits;

        public IReadOnlyDictionary<TLinkAddress, string> CaptureNames()
        {
            PurgeExpired();
            return _named.CaptureNames();
        }

        /// <summary>Whether <paramref name="address"/> is of the store below.</summary>
        public virtual bool Claims(TLinkAddress address) =>
            _links is not IComposedLinks<TLinkAddress> composed || composed.Claims(address);

        /// <summary>Creates the links at <paramref name="addresses"/> that have none, making room for each. The addresses named are not evicted for each other.</summary>
        public virtual void EnsureCreated(params TLinkAddress[] addresses)
        {
            ArgumentNullException.ThrowIfNull(addresses);
            PurgeExpired();
            Settle();
            foreach (var address in addresses)
            {
                if (_entries.ContainsKey(address) || !_constants.IsInternalReference(address))
                {
                    continue;
                }
                MakeRoom(_maxLinks - 1, addresses);
                _links.EnsureCreated(address);
                Changed(null, new[] { address, TLinkAddress.Zero, TLinkAddress.Zero });
            }
        }

        private bool NamesOneLink(IList<TLinkAddress>? restriction)
        {
            if (restriction is null || restriction.Count == 0)
            {
                return false;
            }
            if (restriction[_constants.IndexPart] != _constants.Any)
            {
                return true;
            }
            return restriction.Count >= 3 && restriction[_constants.SourcePart] != _constants.Any && restriction[_constants.TargetPart] != _constants.Any;
        }

        private static InvalidOperationException Full(int? maxLinks = 0) =>
            new($"The cache holds {maxLinks} links and every other one is in use.");

        private WriteHandler<TLinkAddress> Observed(WriteHandler<TLinkAddress>? handler) => (before, after) =>
        {
            Changed(before, after);
            return handler is null ? _constants.Continue : handler(before, after);
        };

        private bool IsNone(IList<TLinkAddress>? link) => link is null || link.Count == 0 || link[_constants.IndexPart] == TLinkAddress.Zero;

        /// <summary>A link the last create made is settled by the next write, keeping nothing but itself.</summary>
        private void Settle() => SettleKeeping(TLinkAddress.Zero, TLinkAddress.Zero);

        private void SettleKeeping(TLinkAddress source, TLinkAddress target)
        {
            if (_unsettled is not { } unsettled)
            {
                return;
            }
            _unsettled = null;
            try
            {
                MakeRoom(_maxLinks, new[] { unsettled, source, target });
            }
            catch (InvalidOperationException)
            {
                _links.Delete(new LinkAddress<TLinkAddress>(unsettled), Observed(null));
                throw;
            }
        }

        private void MakeRoom(int? maxLinks, IList<TLinkAddress> kept)
        {
            if (maxLinks is not { } limit)
            {
                return;
            }
            while (_entries.Count > limit)
            {
                TLinkAddress? victim = null;
                foreach (var candidate in _evictable.Values)
                {
                    if (!kept.Contains(candidate))
                    {
                        victim = candidate;
                        break;
                    }
                }
                if (victim is not { } evicted)
                {
                    throw Full(_maxLinks);
                }
                _links.Delete(new LinkAddress<TLinkAddress>(evicted), Observed(null));
            }
        }

        /// <summary>A change as the store reports it.</summary>
        private void Changed(IList<TLinkAddress>? before, IList<TLinkAddress>? after)
        {
            ForEachReference(before, RemoveUsage);
            ForEachReference(after, AddUsage);
            if (IsNone(before))
            {
                Insert(after![_constants.IndexPart]);
            }
            else if (IsNone(after))
            {
                Remove(before![_constants.IndexPart]);
            }
            else
            {
                Touch(before![_constants.IndexPart]);
            }
        }

        private void ForEachReference(IList<TLinkAddress>? link, Action<TLinkAddress> action)
        {
            if (IsNone(link) || link!.Count < 3)
            {
                return;
            }
            var (index, source, target) = (link[_constants.IndexPart], link[_constants.SourcePart], link[_constants.TargetPart]);
            if (source != index && source != TLinkAddress.Zero)
            {
                action(source);
            }
            if (target != source && target != index && target != TLinkAddress.Zero)
            {
                action(target);
            }
        }

        private void Touch(TLinkAddress link)
        {
            if (!_entries.TryGetValue(link, out var entry))
            {
                return;
            }
            var previous = entry.Used;
            entry.Used = ++_uses;
            if (!_usages.ContainsKey(link))
            {
                _evictable.Remove(previous);
                _evictable[entry.Used] = link;
            }
        }

        private void AddUsage(TLinkAddress link)
        {
            _usages[link] = _usages.GetValueOrDefault(link) + 1;
            if (_usages[link] == 1 && _entries.TryGetValue(link, out var entry))
            {
                _evictable.Remove(entry.Used);
            }
        }

        private void RemoveUsage(TLinkAddress link)
        {
            if (!_usages.TryGetValue(link, out var usages))
            {
                return;
            }
            if (usages > 1)
            {
                _usages[link] = usages - 1;
                return;
            }
            _usages.Remove(link);
            if (_entries.TryGetValue(link, out var entry))
            {
                _evictable[entry.Used] = link;
            }
        }

        private void Insert(TLinkAddress link)
        {
            var entry = new Entry { Used = ++_uses };
            _entries[link] = entry;
            if (!_usages.ContainsKey(link))
            {
                _evictable[entry.Used] = link;
            }
            if (_timeToLive is { } timeToLive)
            {
                SetDeadline(link, _now + Math.Min(timeToLive, ulong.MaxValue - _now));
            }
        }

        private void Remove(TLinkAddress link)
        {
            if (_entries.Remove(link, out var entry))
            {
                _evictable.Remove(entry.Used);
                if (entry.Due is { } due)
                {
                    _deadlines.Remove(due);
                }
            }
        }

        private void SetDeadline(TLinkAddress link, ulong? deadline)
        {
            var entry = _entries[link];
            if (entry.Due is { } previous)
            {
                _deadlines.Remove(previous);
            }
            entry.Due = deadline is { } at ? (at, ++_uses) : null;
            if (entry.Due is { } due)
            {
                _deadlines[due] = link;
            }
        }
    }
}
