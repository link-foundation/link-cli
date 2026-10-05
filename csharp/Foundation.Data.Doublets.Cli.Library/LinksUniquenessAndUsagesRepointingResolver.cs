using System.Numerics;
using Platform.Delegates;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// Keeps every doublet unique: an update that would duplicate an existing link merges into it instead,
    /// re-pointing every link that used the merged-away address at the surviving one.
    /// </summary>
    /// <remarks>
    /// Replaces <see cref="LinksCascadeUniquenessAndUsagesResolver{TLinkAddress}"/>, whose
    /// <c>MergeUsages</c> in Platform.Data.Doublets 0.18.1 writes <c>(index: a, source: b, target: 0)</c>
    /// where it means <c>(source: a, target: b)</c>, so a usage loses its target instead of being re-pointed
    /// (https://github.com/linksplatform/Data.Doublets/issues/515).
    /// </remarks>
    public class LinksUniquenessAndUsagesRepointingResolver<TLinkAddress> : LinksUniquenessResolver<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        public LinksUniquenessAndUsagesRepointingResolver(ILinks<TLinkAddress> links) : base(links) { }

        protected override TLinkAddress ResolveAddressChangeConflict(TLinkAddress oldLinkAddress, TLinkAddress newLinkAddress, WriteHandler<TLinkAddress>? handler)
        {
            WriteHandlerState<TLinkAddress> handlerState = new(_constants.Continue, _constants.Break, handler);
            if (oldLinkAddress != newLinkAddress)
            {
                handlerState.Apply(RepointUsages(oldLinkAddress, newLinkAddress, handlerState.Handler));
            }
            handlerState.Apply(base.ResolveAddressChangeConflict(oldLinkAddress, newLinkAddress, handlerState.Handler));
            return handlerState.Result;
        }

        /// <summary>
        /// Replaces <paramref name="oldLinkAddress"/> with <paramref name="newLinkAddress"/> in every link that uses it,
        /// both halves of a link in one update.
        /// </summary>
        public virtual TLinkAddress RepointUsages(TLinkAddress oldLinkAddress, TLinkAddress newLinkAddress, WriteHandler<TLinkAddress>? handler)
        {
            WriteHandlerState<TLinkAddress> handlerState = new(_constants.Continue, _constants.Break, handler);
            var any = _constants.Any;
            var usages = _facade.All(new Link<TLinkAddress>(any, oldLinkAddress, any))
                .Concat(_facade.All(new Link<TLinkAddress>(any, any, oldLinkAddress)))
                .Select(usage => _facade.GetIndex(usage))
                .Where(usage => usage != oldLinkAddress)
                .Distinct()
                .ToList();
            foreach (var usage in usages)
            {
                // Re-pointing an earlier usage can merge this one away.
                if (!_facade.Exists(usage))
                {
                    continue;
                }
                var link = new Link<TLinkAddress>(_facade.GetLink(usage));
                var source = link.Source == oldLinkAddress ? newLinkAddress : link.Source;
                var target = link.Target == oldLinkAddress ? newLinkAddress : link.Target;
                handlerState.Apply(_facade.Update(new LinkAddress<TLinkAddress>(usage), new Link<TLinkAddress>(usage, source, target), handlerState.Handler));
            }
            return handlerState.Result;
        }
    }

    public static class LinksUniquenessAndUsagesRepointingExtensions
    {
        /// <summary>
        /// The decorators of <c>DecorateWithAutomaticUniquenessAndUsagesResolution</c>, with usages of a merged link
        /// re-pointed by <see cref="LinksUniquenessAndUsagesRepointingResolver{TLinkAddress}"/>.
        /// </summary>
        public static ILinks<TLinkAddress> DecorateWithAutomaticUniquenessAndUsagesRepointing<TLinkAddress>(this ILinks<TLinkAddress> links)
            where TLinkAddress : IUnsignedNumber<TLinkAddress>
        {
            links = new LinksCascadeUsagesResolver<TLinkAddress>(links);
            links = new NonNullContentsLinkDeletionResolver<TLinkAddress>(links);
            return new LinksUniquenessAndUsagesRepointingResolver<TLinkAddress>(links);
        }
    }
}
