using System.Numerics;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Data.Doublets.Decorators;
using System.Collections.Generic;
using Platform.Delegates;
using Platform.Data;
using Platform.Memory;
using Platform.Data.Doublets.Memory;

using System.Diagnostics.CodeAnalysis;

namespace Foundation.Data.Doublets.Cli
{
    public sealed class SimpleLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, IDisposable
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
        private readonly ILinks<TLinkAddress> _namedLinksFacade;
        private bool _disposed;

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
            Justification = "Ownership of the links facade is transferred to the caller, which releases it through " + nameof(Dispose) + ".")]
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

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
            Justification = "The names database memory and links are owned by this instance and released by " + nameof(Dispose) + ".")]
        public SimpleLinksDecorator(ILinks<TLinkAddress> links, string namesDatabaseFilename, bool tracingEnabled = false) : base(links)
        {
            _tracingEnabled = tracingEnabled;
            if (_tracingEnabled) Console.WriteLine($"[Trace] Constructing SimpleLinksDecorator with names DB: {namesDatabaseFilename}");
            var namesConstants = new LinksConstants<TLinkAddress>(enableExternalReferencesSupport: true);
            var namesMemory = new FileMappedResizableDirectMemory(namesDatabaseFilename, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep);
            var namesLinks = new UnitedMemoryLinks<TLinkAddress>(namesMemory, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep, namesConstants, IndexTreeType.Default);
            var decoratedNamesLinks = namesLinks.DecorateWithAutomaticUniquenessAndUsagesResolution();
            _namedLinksFacade = decoratedNamesLinks;
            NamedLinks = new UnicodeStringStorage<TLinkAddress>(decoratedNamesLinks).NamedLinks;
            NamedLinksDatabaseFileName = namesDatabaseFilename;
        }

        public SimpleLinksDecorator(string databaseFilename, bool tracingEnabled = false)
            : this(MakeLinks(databaseFilename), MakeNamesDatabaseFilename(databaseFilename), tracingEnabled)
        {
        }

        /// <summary>
        /// Releases the memory-mapped file handles of both the data and the names databases.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            LinksFacadeDisposer.Dispose(_namedLinksFacade);
            LinksFacadeDisposer.Dispose(_links);
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            var constants = _links.Constants;
            return _links.Delete(restriction, (before, after) => {
                if (handler == null) {
                    return constants.Continue;
                }
                return handler(before, after);
            });
        }
    }
} 