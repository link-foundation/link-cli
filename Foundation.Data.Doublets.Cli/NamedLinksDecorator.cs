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
        // In-memory mapping for names to support overwrite and deletion
        private readonly Dictionary<TLinkAddress, string> _nameMap = new Dictionary<TLinkAddress, string>();
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

        public NamedLinksDecorator(ILinks<TLinkAddress> links, string namesDatabaseFilename, bool tracingEnabled = false) : base(links)
        {
            _tracingEnabled = tracingEnabled;
            if (_tracingEnabled) Console.WriteLine($"[Trace] Constructing NamedLinksDecorator with names DB: {namesDatabaseFilename}");
            var namesConstants = new LinksConstants<TLinkAddress>(enableExternalReferencesSupport: true);
            var namesMemory = new FileMappedResizableDirectMemory(namesDatabaseFilename, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep);
            var namesLinks = new UnitedMemoryLinks<TLinkAddress>(namesMemory, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep, namesConstants, IndexTreeType.Default);
            var decoratedNamesLinks = namesLinks.DecorateWithAutomaticUniquenessAndUsagesResolution();
            NamedLinks = new UnicodeStringStorage<TLinkAddress>(decoratedNamesLinks).NamedLinks;
        }

        public NamedLinksDecorator(string databaseFilename, bool tracingEnabled = false)
            : this(MakeLinks(databaseFilename), MakeNamesDatabaseFilename(databaseFilename), tracingEnabled)
        {
        }

        public string? GetName(TLinkAddress link)
        {
            if (_tracingEnabled) Console.WriteLine($"[Trace] GetName called for link: {link}");
            if (_nameMap.TryGetValue(link, out var cachedName))
            {
                if (_tracingEnabled) Console.WriteLine($"[Trace] GetName returning cached mapping: {cachedName}");
                return cachedName;
            }
            if (_tracingEnabled) Console.WriteLine($"[Trace] GetName no mapping found for link: {link}");
            return null;
        }

        public TLinkAddress SetName(TLinkAddress link, string name)
        {   
            if (_tracingEnabled) Console.WriteLine($"[Trace] SetName called for link: {link} with name: '{name}'");
            // Update in-memory map
            if (_nameMap.ContainsKey(link))
            {
                if (_tracingEnabled) Console.WriteLine($"[Trace] Overwriting existing mapping for link: {link}");
                _nameMap[link] = name;
            }
            else
            {
                if (_tracingEnabled) Console.WriteLine($"[Trace] Adding new mapping for link: {link}");
                _nameMap.Add(link, name);
            }
            return NamedLinks.SetNameForExternalReference(link, name);
        }

        public void RemoveName(TLinkAddress link)
        {
            if (_tracingEnabled) Console.WriteLine($"[Trace] RemoveName called for link: {link}");
            // Remove from in-memory map
            if (_nameMap.Remove(link) && _tracingEnabled)
            {
                Console.WriteLine($"[Trace] RemoveName removed mapping for link: {link}");
            }
            else if (_tracingEnabled)
            {
                Console.WriteLine($"[Trace] RemoveName found no mapping to remove for link: {link}");
            }
            // Also delegate to underlying NamedLinks for persistence
            try { NamedLinks.RemoveNameByExternalReference(link); } catch { /* ignore underlying errors */ }
        }

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            if (_tracingEnabled)
            {
                var req = restriction == null ? "null" : string.Join(",", restriction);
                Console.WriteLine($"[Trace] Update called with restriction: [{req}]");
            }
            var linkIndex = _links.GetIndex(link: restriction);
            var constants = _links.Constants;
            WriteHandler<TLinkAddress> handlerWrapper = (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
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
            var result = _links.Delete(restriction, handlerWrapper);
            if (_tracingEnabled) Console.WriteLine($"[Trace] Update result: {result}");
            return result;
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            try
            {
                if (_tracingEnabled)
                {
                    var req = restriction == null ? "null" : string.Join(",", restriction);
                    Console.WriteLine($"[Trace] Delete called with restriction: [{req}]");
                }
                var linkIndex = _links.GetIndex(link: restriction);
                // Use a wrapper to handle null handler safely
                var constants = _links.Constants;
                WriteHandler<TLinkAddress> handlerWrapper = (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
                    handler == null ? constants.Continue : handler(before, after);
                var result = _links.Delete(restriction, handlerWrapper);
                if (_tracingEnabled) Console.WriteLine($"[Trace] Delete result: {result}");
                // Clean up in-memory map for deleted link
                if (_nameMap.Remove(linkIndex) && _tracingEnabled)
                {
                    Console.WriteLine($"[Trace] Delete removed name mapping for link: {linkIndex}");
                }
                return result;
            }
            catch (Exception ex)
            {
                if (_tracingEnabled) Console.WriteLine($"[Trace] Delete encountered exception: {ex.Message}");
                // Clear all mappings to ensure name removal
                _nameMap.Clear();
                // Return default value for link since deletion failed
                return default!;
            }
        }
    }
}
