using System.Numerics;
using Platform.Delegates;
using Platform.Memory;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;
using System;
using System.Linq;
using System.Collections.Generic;

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
        private readonly bool _tracingEnabled;
        // Tracing flag remains; no in-memory mapping needed
        public readonly NamedLinks<TLinkAddress> NamedLinks;
        public readonly string NamedLinksDatabaseFileName;

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

        public NamedLinksDecorator(ILinks<TLinkAddress> links, string namesDatabaseFilename, bool tracingEnabled = false) : base(links)
        {
            _tracingEnabled = tracingEnabled;
            if (_tracingEnabled) Console.WriteLine($"[Trace] Constructing NamedLinksDecorator with names DB: {namesDatabaseFilename}");
            var namesConstants = new LinksConstants<TLinkAddress>(enableExternalReferencesSupport: true);
            var namesMemory = new FileMappedResizableDirectMemory(namesDatabaseFilename, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep);
            var namesLinks = new UnitedMemoryLinks<TLinkAddress>(namesMemory, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep, namesConstants, IndexTreeType.Default);
            var decoratedNamesLinks = namesLinks.DecorateWithAutomaticUniquenessAndUsagesResolution();
            NamedLinks = new UnicodeStringStorage<TLinkAddress>(decoratedNamesLinks).NamedLinks;
            NamedLinksDatabaseFileName = namesDatabaseFilename;
        }

        public NamedLinksDecorator(string databaseFilename, bool tracingEnabled = false)
            : this(MakeLinks(databaseFilename), MakeNamesDatabaseFilename(databaseFilename), tracingEnabled)
        {
        }

        public string? GetName(TLinkAddress link)
        {
            if (_tracingEnabled) Console.WriteLine($"[Trace] GetName called for link: {link}");
            var result = NamedLinks.GetNameByExternalReference(link);
            if (_tracingEnabled) Console.WriteLine($"[Trace] GetName result: {result}");
            return result;
        }

        public TLinkAddress SetName(TLinkAddress link, string name)
        {   
            if (_tracingEnabled) Console.WriteLine($"[Trace] SetName called for link: {link} with name: '{name}'");
            // Remove any existing name mapping before setting the new one
            RemoveName(link);
            var result = NamedLinks.SetNameForExternalReference(link, name);
            if (_tracingEnabled) Console.WriteLine($"[Trace] SetName result: {result}");
            return result;
        }

        public void RemoveName(TLinkAddress link)
        {
            if (_tracingEnabled) Console.WriteLine($"[Trace] RemoveName called for link: {link}");
            NamedLinks.RemoveNameByExternalReference(link);
            if (_tracingEnabled) Console.WriteLine($"[Trace] RemoveName completed for link: {link}");
        }

        // public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        // {
        //     if (_tracingEnabled)
        //     {
        //         var req = restriction == null ? "null" : string.Join(",", restriction);
        //         Console.WriteLine($"[Trace] Update called with restriction: [{req}]");
        //     }
        //     var linkIndex = _links.GetIndex(link: restriction);
        //     var constants = _links.Constants;
        //     WriteHandler<TLinkAddress> handlerWrapper = (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
        //     {
        //         if (before != null && after == null)
        //         {
        //             var deletedLinkIndex = _links.GetIndex(link: before);
        //             if (deletedLinkIndex == linkIndex)
        //             {
        //                 RemoveName(deletedLinkIndex);
        //             }
        //         }
        //         if (handler == null)
        //         {
        //             return constants.Continue;
        //         }
        //         return handler(before, after);
        //     };
        //     var result = _links.Delete(restriction, handlerWrapper);
        //     if (_tracingEnabled) Console.WriteLine($"[Trace] Update result: {result}");
        //     return result;
        // }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            if (_tracingEnabled)
            {
                var req = restriction == null ? "null" : string.Join(",", restriction);
                Console.WriteLine($"[Trace] Delete called with restriction: [{req}]");
            }
            Console.WriteLine($"[Trace] Debug: this._links is of type: {_links.GetType()}");
            var linkIndex = _links.GetIndex(link: restriction);
            Console.WriteLine($"[Trace] Debug: Computed linkIndex: {linkIndex}");
            var constants = _links.Constants;
            Console.WriteLine($"[Trace] Debug: Retrieved constants type: {constants.GetType()}");
            WriteHandler<TLinkAddress> handlerWrapper = (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
            {
                Console.WriteLine($"[Trace] Debug: handlerWrapper invoked - before={(before==null?"null":string.Join(",", before))}, after={(after==null?"null":string.Join(",", after))}");
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
            Console.WriteLine($"[Trace] Debug: Calling underlying _links.Delete");
            TLinkAddress result;
            try
            {
                result = _links.Delete(restriction: restriction, handler: handlerWrapper);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Trace] Debug: Exception in underlying delete: {ex}");
                throw;
            }
            Console.WriteLine($"[Trace] Debug: Delete result: {result}");
            return result;
        }
    }
}
