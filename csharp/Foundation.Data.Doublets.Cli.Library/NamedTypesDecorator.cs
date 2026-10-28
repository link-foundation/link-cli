using System;
using System.Collections;
using System.Collections.Generic;
using System.Numerics;
using System.Reflection;
using Platform.Delegates;
using Platform.Memory;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;

using System.Diagnostics.CodeAnalysis;

namespace Foundation.Data.Doublets.Cli
{
    public partial class NamedTypesDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, INamedTypesLinks<TLinkAddress>, IPinnedTypes<TLinkAddress>, IDisposable, IDurableLinks
        where TLinkAddress : struct,
            IUnsignedNumber<TLinkAddress>,
            IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
            IShiftOperators<TLinkAddress, int, TLinkAddress>,
            IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
            IMinMaxValue<TLinkAddress>
    {
        private readonly bool _tracingEnabled;
        public readonly PinnedTypesDecorator<TLinkAddress> PinnedTypesDecorator;
        public readonly NamedLinks<TLinkAddress> NamedLinks;
        public readonly string NamedLinksDatabaseFileName;
        private readonly ILinks<TLinkAddress> _namedLinksFacade;
        private bool _disposed;
        private readonly DurableMappedMemory? _dataMemory;
        private readonly DurableMappedMemory? _namesMemory;

        public bool SupportsDurableFlush => _dataMemory is not null;

        public virtual void Flush()
        {
            if (_dataMemory is null || _namesMemory is null)
                throw new InvalidOperationException("A caller-owned store needs an explicit flush callback.");
            _dataMemory.Flush();
            _namesMemory.Flush();
        }

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
            Justification = "Ownership of the links facade is transferred to the caller, which releases it through " + nameof(Dispose) + ".")]
        public static ILinks<TLinkAddress> MakeLinks(string databaseFilename)
        {
            return CreateOwnedStore(databaseFilename).Links;
        }

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
            Justification = "The returned decorator owns its UnitedMemoryLinks and mapping; LinksFacadeDisposer releases the chain.")]
        private static (ILinks<TLinkAddress> Links, DurableMappedMemory Memory) CreateOwnedStore(string filename)
        {
            var memory = new DurableMappedMemory(filename, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep);
            try
            {
                var links = new UnitedMemoryLinks<TLinkAddress>(memory, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep,
                    new LinksConstants<TLinkAddress>(), IndexTreeType.Default);
                return (links.DecorateWithAutomaticUniquenessAndUsagesRepointing(), memory);
            }
            catch { memory.Dispose(); throw; }
        }

        public static string MakeNamesDatabaseFilename(string databaseFilename)
        {
            var filenameWithoutExtension = Path.GetFileNameWithoutExtension(databaseFilename);
            var directory = Path.GetDirectoryName(databaseFilename);
            var namesDatabaseFilename = Path.Combine(directory ?? string.Empty, $"{filenameWithoutExtension}.names.links");
            return namesDatabaseFilename;
        }

        public NamedTypesDecorator(ILinks<TLinkAddress> links, string namesDatabaseFilename, bool tracingEnabled = false)
            : this(new PinnedTypesDecorator<TLinkAddress>(links), namesDatabaseFilename, tracingEnabled)
        {
        }

        public NamedTypesDecorator(PinnedTypesDecorator<TLinkAddress> pinnedTypesDecorator, string namesDatabaseFilename, bool tracingEnabled = false)
            : this(pinnedTypesDecorator, namesDatabaseFilename, tracingEnabled, null)
        {
        }

        private NamedTypesDecorator(PinnedTypesDecorator<TLinkAddress> pinnedTypesDecorator,
            string namesDatabaseFilename, bool tracingEnabled, DurableMappedMemory? ownedDataMemory) : base(pinnedTypesDecorator)
        {
            _tracingEnabled = tracingEnabled;
            PinnedTypesDecorator = pinnedTypesDecorator;
            _dataMemory = ownedDataMemory;
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Constructing NamedTypesDecorator with names DB: {namesDatabaseFilename}");
            try
            {
                _namesMemory = new DurableMappedMemory(namesDatabaseFilename, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep);
                (_namedLinksFacade, NamedLinks) = NamesDatabase<TLinkAddress>.Open(_namesMemory);
            }
            catch
            {
                if (ownedDataMemory is not null) LinksFacadeDisposer.Dispose(pinnedTypesDecorator);
                throw;
            }
            NamedLinksDatabaseFileName = namesDatabaseFilename;
        }

        public NamedTypesDecorator(string databaseFilename, bool tracingEnabled = false)
            : this(CreateOwnedStore(databaseFilename), MakeNamesDatabaseFilename(databaseFilename), tracingEnabled)
        {
        }

        private NamedTypesDecorator((ILinks<TLinkAddress> Links, DurableMappedMemory Memory) store,
            string namesFilename, bool tracingEnabled)
            : this(PinOwnedStore(store.Links), namesFilename, tracingEnabled, store.Memory)
        {
        }

        private static PinnedTypesDecorator<TLinkAddress> PinOwnedStore(ILinks<TLinkAddress> links)
        {
            try { return new PinnedTypesDecorator<TLinkAddress>(links); }
            catch { LinksFacadeDisposer.Dispose(links); throw; }
        }

        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        /// <summary>
        /// Releases the names database this decorator owns. Derived decorators
        /// that own extra resources override this and call
        /// <c>base.Dispose(disposing)</c>.
        /// </summary>
        protected virtual void Dispose(bool disposing)
        {
            if (_disposed) return;
            _disposed = true;
            if (!disposing) return;
            LinksFacadeDisposer.Dispose(_namedLinksFacade);
            LinksFacadeDisposer.Dispose(PinnedTypesDecorator);
        }

        public virtual IEnumerator<TLinkAddress> GetEnumerator()
        {
            return PinnedTypesDecorator.GetEnumerator();
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
        }

        public virtual void Deconstruct(out TLinkAddress type1, out TLinkAddress type2, out TLinkAddress type3)
        {
            PinnedTypesDecorator.Deconstruct(out type1, out type2, out type3);
        }

        public virtual IReadOnlyDictionary<TLinkAddress, string> CaptureNames() => NamedLinks.ExternalNames();

        public virtual string? GetName(TLinkAddress link)
        {
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] GetName called for link: {link}");
            var result = NamedLinks.GetNameByExternalReference(link);
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] GetName result: {result}");
            return result;
        }

        public virtual TLinkAddress SetName(TLinkAddress link, string name)
        {
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] SetName called for link: {link} with name: '{name}'");
            if (NamedLinks.TryGetExternalReferenceByName(name, out var existingLinkWithName) && !existingLinkWithName.Equals(link))
            {
                RemoveName(existingLinkWithName);
            }
            RemoveName(link);
            var result = NamedLinks.SetNameForExternalReference(link, name);
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] SetName result: {result}");
            return result;
        }

        public virtual TLinkAddress GetByName(string name)
        {
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] GetByName called for name: '{name}'");
            var result = NamedLinks.GetExternalReferenceByName(name);
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] GetByName result: {result}");
            return result;
        }

        public virtual void RemoveName(TLinkAddress link)
        {
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] RemoveName called for link: {link}");
            NamedLinks.RemoveNameByExternalReference(link);
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] RemoveName completed for link: {link}");
        }

        public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
        {
            if (_tracingEnabled)
            {
                var req = restriction == null ? "null" : string.Join(",", restriction);
                DiagnosticOutput.WriteLine($"[Trace] Update called with restriction: [{req}]");
            }
            var @continue = _links.Constants.Continue;
            var result = _links.Update(restriction, substitution, (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
            {
                if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Debug: handlerWrapper invoked - before={(before == null ? "null" : string.Join(",", before))}, after={(after == null ? "null" : string.Join(",", after))}");
                if (before != null && after == null)
                {
                    var deletedLinkIndex = _links.GetIndex(link: before);
                    RemoveName(deletedLinkIndex);
                }
                return handler == null ? @continue : handler(before, after);
            });
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Update result: {result}");
            return result;
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            if (_tracingEnabled)
            {
                var formattedRestriction = restriction == null ? "null" : string.Join(",", restriction);
                DiagnosticOutput.WriteLine($"[Trace] Delete called with restriction: [{formattedRestriction}]");
            }
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Debug: this._links is of type: {_links.GetType()}");
            var @continue = _links.Constants.Continue;
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Debug: Calling underlying _links.Delete");
            TLinkAddress result = _links.Delete(restriction, (IList<TLinkAddress>? before, IList<TLinkAddress>? after) =>
            {
                if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Debug: handlerWrapper invoked - before={(before == null ? "null" : string.Join(",", before))}, after={(after == null ? "null" : string.Join(",", after))}");
                if (before != null && after == null)
                {
                    var deletedLinkIndex = _links.GetIndex(link: before);
                    RemoveName(deletedLinkIndex);
                }
                return handler == null ? @continue : handler(before, after);
            });
            if (_tracingEnabled) DiagnosticOutput.WriteLine($"[Trace] Debug: Delete result: {result}");
            return result;
        }
    }
}
