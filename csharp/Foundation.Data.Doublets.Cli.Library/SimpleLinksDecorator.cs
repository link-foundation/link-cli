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
    public class SimpleLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, IDisposable
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
            try { return links.DecorateWithAutomaticUniquenessAndUsagesRepointing(); }
            catch { LinksFacadeDisposer.Dispose(links); throw; }
        }

        public static string MakeNamesDatabaseFilename(string databaseFilename)
        {
            var filenameWithoutExtension = Path.GetFileNameWithoutExtension(databaseFilename);
            var directory = Path.GetDirectoryName(databaseFilename);
            var namesDatabaseFilename = Path.Combine(directory ?? string.Empty, $"{filenameWithoutExtension}.names.links");
            return namesDatabaseFilename;
        }

        public SimpleLinksDecorator(ILinks<TLinkAddress> links, string namesDatabaseFilename, bool tracingEnabled = false)
            : this(links, namesDatabaseFilename, tracingEnabled, ownsData: false)
        {
        }

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
            Justification = "NamesDatabase.Open disposes on failure and transfers the mapping to the owned names facade on success.")]
        private SimpleLinksDecorator(ILinks<TLinkAddress> links, string namesDatabaseFilename,
            bool tracingEnabled, bool ownsData) : base(links)
        {
            _tracingEnabled = tracingEnabled;
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Constructing SimpleLinksDecorator with names DB: {namesDatabaseFilename}");
            try
            {
                var namesMemory = new FileMappedResizableDirectMemory(namesDatabaseFilename, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep);
                (_namedLinksFacade, NamedLinks) = NamesDatabase<TLinkAddress>.Open(namesMemory);
            }
            catch
            {
                if (ownsData) LinksFacadeDisposer.Dispose(links);
                throw;
            }
            NamedLinksDatabaseFileName = namesDatabaseFilename;
        }

        public SimpleLinksDecorator(string databaseFilename, bool tracingEnabled = false)
            : this(MakeLinks(databaseFilename), MakeNamesDatabaseFilename(databaseFilename), tracingEnabled, ownsData: true)
        {
        }

        /// <summary>
        /// Releases the memory-mapped file handles of both the data and the names databases.
        /// </summary>
        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        /// <summary>
        /// Releases the databases this decorator owns. Derived decorators that
        /// own extra resources override this and call
        /// <c>base.Dispose(disposing)</c>.
        /// </summary>
        protected virtual void Dispose(bool disposing)
        {
            if (_disposed) return;
            _disposed = true;
            if (!disposing) return;
            LinksFacadeDisposer.Dispose(_namedLinksFacade);
            LinksFacadeDisposer.Dispose(_links);
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            var constants = _links.Constants;
            return _links.Delete(restriction, (before, after) =>
            {
                if (handler == null)
                {
                    return constants.Continue;
                }
                return handler(before, after);
            });
        }
    }
}
