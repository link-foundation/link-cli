using System;
using System.Security.Cryptography;
using System.Text;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// The tokens a links server knows, stored as links (issue #149): the
    /// authenticator of <c>(auth: token)</c> that <c>clink --tokens</c> uses.
    /// </summary>
    /// <remarks>
    /// A subject <c>s</c> of the data has a pair <c>(p: Token s)</c>, where
    /// <c>Token</c> is the point named <c>Token</c>, and each of its tokens is a
    /// link <c>(t: p t)</c>, named <c>sha256:&lt;hex&gt;</c> after the SHA-256 of the
    /// token, so the store never holds a token itself, and a subject may have
    /// many (a store of unique doublets would merge equal links). The table
    /// lives in a store of its own or in the data; any other authenticator is
    /// a <see cref="Func{T, TResult}"/> of
    /// <see cref="Protocol.LinksServerOptions.Authenticate"/>.
    /// </remarks>
    public sealed class LinksTokenTable
    {
        /// <summary>The name of the point every token link starts from.</summary>
        public const string TokenName = "Token";

        private readonly INamedTypesLinks<uint> _store;

        /// <param name="store">The store of the table.</param>
        public LinksTokenTable(INamedTypesLinks<uint> store)
        {
            ArgumentNullException.ThrowIfNull(store);
            _store = store;
        }

        /// <summary>The name of the link of <paramref name="token"/>.</summary>
        public static string NameOf(string token)
        {
            ArgumentNullException.ThrowIfNull(token);
            return "sha256:" + Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(token))).ToLowerInvariant();
        }

        /// <summary>The subject of <paramref name="token"/>, or null when the table does not know it.</summary>
        public uint? Authenticate(string token)
        {
            ArgumentNullException.ThrowIfNull(token);
            var none = _store.Constants.Null;
            var point = _store.GetByName(TokenName);
            var link = _store.GetByName(NameOf(token));
            if (point == none || link == none || !_store.Exists(link) || _store.GetTarget(link) != link)
            {
                return null;
            }
            var pair = _store.GetSource(link);
            if (pair == none || pair == link || !_store.Exists(pair) || _store.GetSource(pair) != point)
            {
                return null;
            }
            var subject = _store.GetTarget(pair);
            return subject == none ? null : subject;
        }

        /// <summary>Lets <paramref name="token"/> authenticate as <paramref name="subject"/>, instead of any subject it had.</summary>
        /// <returns>The link of the token.</returns>
        public uint Grant(string token, uint subject)
        {
            ArgumentNullException.ThrowIfNull(token);
            if (subject == _store.Constants.Null)
            {
                throw new ArgumentException("A token needs a subject", nameof(subject));
            }
            var point = TypesDecorator<uint>.EnsurePoint(_store, _store, TokenName);
            var pair = _store.SearchOrDefault(point, subject);
            if (pair == _store.Constants.Null)
            {
                pair = _store.Create();
                _store.Update(pair, point, subject);
            }
            var name = NameOf(token);
            var link = _store.GetByName(name);
            if (link == _store.Constants.Null || !_store.Exists(link))
            {
                link = _store.Create();
            }
            _store.Update(link, pair, link);
            _store.SetName(link, name);
            return link;
        }

        /// <summary>Forgets <paramref name="token"/>; whether the table knew it.</summary>
        public bool Revoke(string token)
        {
            ArgumentNullException.ThrowIfNull(token);
            if (Authenticate(token) is null)
            {
                return false;
            }
            var link = _store.GetByName(NameOf(token));
            _store.RemoveName(link);
            _store.Delete(link);
            return true;
        }
    }
}
