using System;
using System.Collections;
using System.Collections.Generic;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    public class PinnedTypes<TLinkAddress> : IEnumerable<TLinkAddress>
        where TLinkAddress : struct, System.Numerics.IUnsignedNumber<TLinkAddress>
    {
        private readonly ILinks<TLinkAddress> _links;

        public PinnedTypes(ILinks<TLinkAddress> links)
        {
            _links = links;
        }

        public IEnumerator<TLinkAddress> GetEnumerator()
        {
            return new PinnedTypesEnumerator(_links);
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
        }

        // Private custom enumerator
        private class PinnedTypesEnumerator : IEnumerator<TLinkAddress>
        {
            private readonly ILinks<TLinkAddress> _links;
            private readonly TLinkAddress _initialSource;
            private TLinkAddress _currentAddress;

            public PinnedTypesEnumerator(ILinks<TLinkAddress> links)
            {
                _links = links;
                _initialSource = TLinkAddress.One;
                _currentAddress = TLinkAddress.One; // Start with the first address
            }

            public TLinkAddress Current { get; private set; }

            object IEnumerator.Current => Current;

            public bool MoveNext()
            {
                if (_links.Exists(_currentAddress))
                {
                    var link = new Link<TLinkAddress>(_links.GetLink(_currentAddress));
                    var expectedLink = new Link<TLinkAddress>(_currentAddress, _initialSource, _currentAddress);
                    if (link == expectedLink)
                    {
                        // Link already exists and matches the expected structure
                        Current = _currentAddress;
                    }
                    else
                    {
                        // Link exists but does not match the expected structure
                        throw new InvalidOperationException($"Unexpected link found at address {_currentAddress}. Expected: {expectedLink}, Found: {link}.");
                    }
                }
                else
                {
                    // Create a new link if none exists
                    Current = _links.GetOrCreate(_initialSource, _currentAddress);
                }

                // Increment the current address for the next type
                _currentAddress++;

                return true;
            }

            public void Reset()
            {
                _currentAddress = TLinkAddress.One;
            }

            public void Dispose()
            {
                // No resources to dispose
            }
        }
    }
}