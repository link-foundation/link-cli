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
        private readonly TLinkAddress _initialSource;
        private readonly int _numberOfTypes;

        public PinnedTypes(ILinks<TLinkAddress> links, TLinkAddress initialSource, int numberOfTypes)
        {
            _links = links;
            _initialSource = initialSource;
            _numberOfTypes = numberOfTypes;
        }

        public IEnumerator<TLinkAddress> GetEnumerator()
        {
            return new PinnedTypesEnumerator(_links, _initialSource, _numberOfTypes);
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
            private readonly int _numberOfTypes;
            private int _currentIndex;
            private TLinkAddress _currentAddress;

            public PinnedTypesEnumerator(ILinks<TLinkAddress> links, TLinkAddress initialSource, int numberOfTypes)
            {
                _links = links;
                _initialSource = initialSource;
                _numberOfTypes = numberOfTypes;
                _currentIndex = -1; // Start before the first element
                _currentAddress = TLinkAddress.One; // Start with the first address
            }

            public TLinkAddress Current { get; private set; }

            object IEnumerator.Current => Current;

            public bool MoveNext()
            {
                _currentIndex++;
                if (_currentIndex >= _numberOfTypes)
                {
                    return false; // No more types to iterate
                }
                
                var expectedLink = new Link<TLinkAddress>(_currentAddress, _initialSource, _currentAddress);

                // Check if the link already exists
                var searchResult = _links.SearchOrDefault(expectedLink.Source, expectedLink.Target);

                if (!EqualityComparer<TLinkAddress>.Default.Equals(searchResult, default))
                {
                    var link = new Link<TLinkAddress>(_links.GetLink(_currentAddress));
                    if (searchResult == _currentAddress)
                    {
                        // Link already exists, no need to create a new one
                        Current = searchResult;
                    }
                    else
                    {
                        // Link exists but is not the expected one
                        throw new InvalidOperationException($"Unexpected link found at address {_currentAddress}. Expected: {expectedLink}, Found: {link}.");
                    }
                }
                if (EqualityComparer<TLinkAddress>.Default.Equals(searchResult, default) && _links.Exists(_currentAddress))
                {
                    var link = new Link<TLinkAddress>(_links.GetLink(_currentAddress));
                    if (link.Source == _initialSource && link.Target == _currentAddress)
                    {
                        // Link already exists, no need to create a new one
                        Current = searchResult;
                    }
                    else
                    {
                        // Link exists but is not the expected one
                        throw new InvalidOperationException($"Unexpected link found at address {_currentAddress}. Expected: {expectedLink}, Found: {link}.");
                    }
                }
                else
                {
                    // Create a new link if none exists
                    Current = _links.GetOrCreate(_initialSource, _currentAddress);
                }

                // Increment the current address for the next type
                _currentAddress = IncrementAddress(_currentAddress);

                return true;
            }

            public void Reset()
            {
                _currentIndex = -1;
                _currentAddress = TLinkAddress.One;
            }

            public void Dispose()
            {
                // No resources to dispose
            }

            // Helper method to increment the address
            private TLinkAddress IncrementAddress(TLinkAddress address)
            {
                dynamic addr = address;
                return (TLinkAddress)(addr + 1);
            }
        }
    }
}