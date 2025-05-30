using System.Numerics;
using Platform.Delegates;
using Platform.Memory;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;

namespace Foundation.Data.Doublets.Cli
{
    public class NamedLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>
        where TLinkAddress : struct,
            IUnsignedNumber<TLinkAddress>,
            IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
            IShiftOperators<TLinkAddress, int, TLinkAddress>,
            IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
            IMinMaxValue<TLinkAddress>
    {
        public NamedLinks<TLinkAddress> NamedLinks;

        public static ILinks<TLinkAddress> MakeLinks(string databaseFilename)
        {
            var links = new UnitedMemoryLinks<TLinkAddress>(databaseFilename);
            return links.DecorateWithAutomaticUniquenessAndUsagesResolution();
        }

        public static string MakeNamesDatabaseFilename(string databaseFilename)
        {
            var filenameWithoutExtension = Path.GetFileNameWithoutExtension(databaseFilename);
            var directory = Path.GetDirectoryName(databaseFilename);
            var namesDatabaseFilename = Path.Combine(directory ?? string.Empty, $"{filenameWithoutExtension}.names.links");
            return namesDatabaseFilename;
        }

        public NamedLinksDecorator(ILinks<TLinkAddress> links, string namesDatabaseFilename) : base(links)
        {
            var namesConstants = new LinksConstants<TLinkAddress>(enableExternalReferencesSupport: true);
            var namesMemory = new FileMappedResizableDirectMemory(namesDatabaseFilename, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep);
            var namesLinks = new UnitedMemoryLinks<TLinkAddress>(namesMemory, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep, namesConstants, IndexTreeType.Default);
            var decoratedNamesLinks = namesLinks.DecorateWithAutomaticUniquenessAndUsagesResolution();
            NamedLinks = new UnicodeStringStorage<TLinkAddress>(decoratedNamesLinks).NamedLinks;
        }

        public NamedLinksDecorator(string databaseFilename): this(MakeLinks(databaseFilename), MakeNamesDatabaseFilename(databaseFilename))
        {
        }

        public string? GetName(TLinkAddress link)
        {
            return NamedLinks.GetNameByExternalReference(link);
        }

        public TLinkAddress SetName(TLinkAddress link, string name)
        {   
            return NamedLinks.SetNameForExternalReference(link, name);
        }

        public void RemoveName(TLinkAddress link)
        {
            return NamedLinks.RemoveNameByExternalReference(link);
        }

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            var linkIndex = _links.GetIndex(link: restriction);
            var constants = _links.Constants;
            var handlerWrapper = (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
            {
                if (before != null && after == null)
                {
                    var deletedLinkIndex = _links.GetIndex(link: before);
                    if (deletedLinkIndex == linkIndex)
                    {
                        RemoveName(deletedLinkIndex);
                    }
                }
                if (handler == null)
                {
                    return constants.Continue;
                }
                return handler(before, after);
            };
            return _links.Delete(restriction, handlerWrapper);
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            var linkIndex = _links.GetIndex(link: restriction);
            var constants = _links.Constants;
            var handlerWrapper = (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
            {
                if (before != null && after == null)
                {
                    var deletedLinkIndex = _links.GetIndex(link: before);
                    if (deletedLinkIndex == linkIndex)
                    {
                        RemoveName(deletedLinkIndex);
                    }
                }
                if (handler == null)
                {
                    return constants.Continue;
                }
                return handler(before, after);
            };
            return _links.Delete(restriction, handlerWrapper);
        }
    }
}
