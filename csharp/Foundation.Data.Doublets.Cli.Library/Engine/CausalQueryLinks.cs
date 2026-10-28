using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class CausalSession
{
    private QueryReservations<ulong>? _queryReservations;
    private CheckedQueryLinks? _queryLinks;
    /// <summary>Checked ordinary query API. UInt128 keeps Any outside the stored u64 domain.</summary>
    public INamedTypesLinks<UInt128> QueryLinks
    {
        get { _ = Active; return _queryLinks ??= new(this); }
    }
    private sealed class CheckedQueryLinks(CausalSession owner) : INamedTypesLinks<UInt128>
    {
        private static readonly QueryLimits Limits = new();
        public LinksConstants<UInt128> Constants { get; } = new((UInt128.One, UInt128.MaxValue));
        public QueryLimits QueryLimits { get { _ = owner.Active; return Limits; } }
        private static ulong Address(UInt128 value, bool nonzero = false)
        {
            if (value > ulong.MaxValue || (nonzero && value == 0)) throw new ArgumentOutOfRangeException(nameof(value));
            return (ulong)value;
        }
        private static LinkParts<UInt128> Parts(Link<ulong> link) => new(link.Index, link.Source, link.Target);
        private IEnumerable<EngineRow<UInt128>> Rows() => owner.Rows().Select(row => new EngineRow<UInt128>(
            new(row.Link.Index, row.Link.Source, row.Link.Target), row.Name));
        private Link<UInt128>[] Matching(IList<UInt128>? restriction)
        {
            _ = owner.Active;
            if (restriction is not null) foreach (var part in restriction) if (part != Constants.Any) Address(part);
            return QueryMatching.Rows(Rows().Select(row => row.Link), restriction, Constants.Any, Limits);
        }
        public UInt128 Count(IList<UInt128>? restriction) => (UInt128)Matching(restriction).Length;
        public UInt128 Each(IList<UInt128>? restriction, ReadHandler<UInt128>? handler)
        {
            foreach (var row in Matching(restriction))
                if (handler?.Invoke(new LinkParts<UInt128>(row.Index, row.Source, row.Target)) == Constants.Break) return Constants.Break;
            return Constants.Continue;
        }
        public string? GetName(UInt128 id) { _ = owner.Active; return owner.GetName(Address(id)); }
        public UInt128 GetByName(string name) { _ = owner.Active; return owner.GetByName(name); }
        public UInt128 SetName(UInt128 id, string name) { _ = owner.Active; return owner.SetName(Address(id, true), name); }
        public void RemoveName(UInt128 id) { _ = owner.Active; owner.RemoveName(Address(id, true)); }
        public bool TryEnsureCreated(UInt128 id) { _ = owner.Active; owner.EnsureCreated(Address(id, true)); return true; }
        public UInt128 Create(IList<UInt128>? substitution, WriteHandler<UInt128>? handler)
        {
            _ = owner.Active; var id = owner.CreateLink(0, 0);
            return handler?.Invoke(null, new LinkParts<UInt128>(id, 0, 0)) ?? Constants.Continue;
        }
        public UInt128 Update(IList<UInt128>? restriction, IList<UInt128>? substitution, WriteHandler<UInt128>? handler)
        {
            _ = owner.Active;
            if (restriction is not [var id, ..] || substitution is not [_, var source, var target])
                throw new ArgumentException("An update needs an index and a triple.");
            var index = Address(id, true); var left = Address(source); var right = Address(target);
            var before = owner.UpdateLink(index, left, right);
            return handler?.Invoke(Parts(before), new LinkParts<UInt128>(index, left, right)) ?? Constants.Continue;
        }
        public UInt128 Delete(IList<UInt128>? restriction, WriteHandler<UInt128>? handler)
        {
            _ = owner.Active;
            if (restriction is not [var id, ..]) throw new ArgumentException("A delete needs an index.");
            var before = owner.DeleteLink(Address(id, true));
            return handler?.Invoke(Parts(before), null) ?? Constants.Continue;
        }
        public IReadOnlySet<UInt128>? ReserveQueryAllocations(Action<INamedTypesLinks<UInt128>> preview)
        {
            _ = owner.Active; ArgumentNullException.ThrowIfNull(preview);
            var scope = new QueryAddressScope<UInt128>(owner._first, owner._last, owner._retained.Keys.Select(id => (UInt128)id));
            var scratch = new QueryPreview<UInt128>(Rows(), owner._next,
                owner._queryReservations?.Addresses.Select(id => (UInt128)id) ?? [], Constants, scope, Limits);
            preview(scratch);
            owner._next = BigInteger.Max(owner._next, scratch.Next);
            foreach (var id in scratch.Allocated) (owner._queryReservations ??= new()).Add((ulong)id);
            return scratch.Rows.Keys.ToHashSet();
        }
    }
}
