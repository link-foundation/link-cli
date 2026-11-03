using System;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// The ids other systems give our links, stored as links (issue #149): a
    /// UUID of a service, an ObjectId of a document database, a URL, bound to
    /// a link of ours and back, so the code that talks to the other system never
    /// keeps a table of its own. The Rust counterpart is
    /// <c>link_cli::LinksExternalIds</c>; in JavaScript a <c>RemapLinks</c>
    /// reads the links under such ids directly.
    /// </summary>
    /// <remarks>
    /// The ids of one kind share a scheme, such as <c>uuid</c>. The point named
    /// <c>ExternalId</c> is <c>E</c>, the scheme is the link <c>(S: E S)</c> named
    /// after it, and a link <c>l</c> bound to the id <c>x</c> has a pair
    /// <c>(p: S l)</c> named <c>scheme:x</c>, so a link has one id of a scheme and
    /// an id one link. No link of the table but a pair starts at <c>S</c>, so a
    /// pair is never a link of the table itself, whatever address <c>l</c> has.
    /// The table lives in the data or in a store of its own, as the rules of
    /// permissions and the tokens of <see cref="LinksTokenTable"/> do; in a store
    /// of its own every address it holds but its own is an address of the data.
    /// </remarks>
    public sealed class LinksExternalIds
    {
        /// <summary>The name of the point every scheme starts from.</summary>
        public const string ExternalIdName = "ExternalId";

        private readonly INamedTypesLinks<uint> _store;

        /// <param name="store">The store of the table: the data or a store of its own.</param>
        /// <param name="scheme">The kind of the ids, such as <c>uuid</c>.</param>
        public LinksExternalIds(INamedTypesLinks<uint> store, string scheme = "id")
        {
            ArgumentNullException.ThrowIfNull(store);
            ArgumentException.ThrowIfNullOrEmpty(scheme);
            if (scheme.Contains(':', StringComparison.Ordinal))
            {
                throw new ArgumentException("A scheme has no colon.", nameof(scheme));
            }
            _store = store;
            Scheme = scheme;
        }

        /// <summary>The kind of the ids.</summary>
        public string Scheme { get; }

        /// <summary>The name of the pair of <paramref name="id"/>.</summary>
        public string NameOf(string id)
        {
            ArgumentException.ThrowIfNullOrEmpty(id);
            return Scheme + ":" + id;
        }

        /// <summary>The link <paramref name="id"/> is bound to, null when it is bound to none.</summary>
        public uint? Find(string id)
        {
            var pair = PairOf(id);
            return pair is { } found ? _store.GetTarget(found) : null;
        }

        /// <summary>The id <paramref name="link"/> is bound to, null when it has none.</summary>
        public string? IdOf(uint link)
        {
            var point = SchemeLink();
            if (point == 0 || link == 0)
            {
                return null;
            }
            var pair = _store.SearchOrDefault(point, link);
            var name = pair == 0 ? null : _store.GetName(pair);
            var prefix = Scheme + ":";
            return name is not null && name.StartsWith(prefix, StringComparison.Ordinal) ? name[prefix.Length..] : null;
        }

        /// <summary>Binds <paramref name="id"/> to <paramref name="link"/>, instead of any link it had and of any id the link had.</summary>
        /// <returns>The pair that binds them.</returns>
        public uint Bind(string id, uint link)
        {
            var name = NameOf(id);
            if (link == 0)
            {
                throw new ArgumentException("The null link has no id.", nameof(link));
            }
            Unbind(id);
            var point = SchemeLink();
            if (point == 0)
            {
                var root = TypesDecorator<uint>.EnsurePoint(_store, _store, ExternalIdName);
                point = _store.Create();
                _store.Update(point, root, point);
                _store.SetName(point, Scheme);
            }
            var pair = _store.SearchOrDefault(point, link);
            if (pair == 0)
            {
                pair = _store.Create();
                _store.Update(pair, point, link);
            }
            _store.SetName(pair, name);
            return pair;
        }

        /// <summary>The link <paramref name="id"/> is bound to, a new point <c>(l: l l)</c> of <paramref name="data"/> bound to it when it has none.</summary>
        public uint Ensure(string id, ILinks<uint> data)
        {
            ArgumentNullException.ThrowIfNull(data);
            if (Find(id) is { } found)
            {
                return found;
            }
            var link = data.Create();
            data.Update(link, link, link);
            Bind(id, link);
            return link;
        }

        /// <summary>Unbinds <paramref name="id"/>; whether it was bound.</summary>
        public bool Unbind(string id)
        {
            if (PairOf(id) is not { } pair)
            {
                return false;
            }
            _store.RemoveName(pair);
            _store.Delete(pair);
            return true;
        }

        private uint? PairOf(string id)
        {
            var name = NameOf(id);
            var point = SchemeLink();
            var pair = _store.GetByName(name);
            if (point == 0 || pair == 0 || !_store.Exists(pair) || _store.GetSource(pair) != point)
            {
                return null;
            }
            return pair;
        }

        /// <summary>The link <c>(S: E S)</c> of the scheme, 0 when the table has none yet.</summary>
        private uint SchemeLink()
        {
            var root = _store.GetByName(ExternalIdName);
            var point = _store.GetByName(Scheme);
            return root != 0 && point != 0 && _store.Exists(point) && _store.GetSource(point) == root && _store.GetTarget(point) == point ? point : 0;
        }
    }
}
