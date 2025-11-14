using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    public class NamedLinks<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        private readonly ILinks<TLinkAddress> _links;
        private readonly TLinkAddress _nameType;
        private readonly Func<string, TLinkAddress> _createString;
        private readonly Func<TLinkAddress, string> _getString;

        public NamedLinks(
            ILinks<TLinkAddress> links,
            TLinkAddress nameType,
            Func<string, TLinkAddress> createString,
            Func<TLinkAddress, string> getString)
        {
            _links = links;
            _nameType = nameType;
            _createString = createString;
            _getString = getString;
        }

        public TLinkAddress SetNameForExternalReference(TLinkAddress link, string name)
        {
            var reference = new Hybrid<TLinkAddress>(link, isExternal: true);
            return SetName(reference, name);
        }

        public TLinkAddress SetName(TLinkAddress link, string name)
        {
            var nameSequence = _createString(name);
            return _links.GetOrCreate(link, _links.GetOrCreate(_nameType, nameSequence));
        }

        public string? GetNameByExternalReference(TLinkAddress link)
        {
            var reference = new Hybrid<TLinkAddress>(link, isExternal: true);
            return GetName(reference);
        }

        public string? GetName(TLinkAddress link)
        {
            var any = _links.Constants.Any;
            var query = new Link<TLinkAddress>(any, link, any);
            var nameCandidatesPairs = _links.All(query);
            foreach (var nameCandidatePair in nameCandidatesPairs)
            {
                var nameCandidate = _links.GetTarget(nameCandidatePair);
                if (_links.GetSource(nameCandidate).Equals(_nameType))
                {
                    var strLink = _links.GetTarget(nameCandidate);
                    return _getString(strLink);
                }
            }
            return null;
        }

        public TLinkAddress GetByName(string name)
        {
            var nameSequence = _createString(name);
            var nameLink = _links.SearchOrDefault(_nameType, nameSequence);
            if (nameLink.Equals(_links.Constants.Null))
            {
                return _links.Constants.Null;
            }
            var any = _links.Constants.Any;
            var query = new Link<TLinkAddress>(any, any, nameLink);
            var link = _links.All(query).SingleOrDefault();
            if (link == null)
            {
                return _links.Constants.Null;
            }
            return _links.GetSource(link);
        }

        public TLinkAddress GetExternalReferenceByName(string name)
        {
            var reference = (Hybrid<TLinkAddress>)GetByName(name);
            if (reference.IsExternal)
            {
                return TLinkAddress.CreateTruncating(reference.AbsoluteValue);
            }
            else
            {
                return _links.Constants.Null;
            }
        }

        public void RemoveName(TLinkAddress link)
        {
            var any = _links.Constants.Any;
            var query = new Link<TLinkAddress>(any, link, any);
            var nameCandidatesPairs = _links.All(query).ToList();
            foreach (var nameCandidatePair in nameCandidatesPairs)
            {
                var nameCandidate = _links.GetTarget(nameCandidatePair);
                if (_links.GetSource(nameCandidate).Equals(_nameType))
                {
                    var nameSequence = _links.GetTarget(nameCandidate);
                    var nameTypeToNameSequenceLink = nameCandidate;

                    // Check if this nameType->nameSequence is used elsewhere BEFORE deleting
                    var queryNameType = new Link<TLinkAddress>(any, any, nameTypeToNameSequenceLink);
                    var usagesOfNameCandidate = _links.All(queryNameType).ToList();

                    // Remove the name link (link->nameCandidate)
                    _links.Delete(nameCandidatePair);

                    // Remove the nameType->nameSequence link if this was the only usage
                    if (usagesOfNameCandidate.Count == 1)
                    {
                        _links.Delete(nameTypeToNameSequenceLink);
                    }
                }
            }
        }

        public void RemoveNameByExternalReference(TLinkAddress externalReference)
        {
            var reference = new Hybrid<TLinkAddress>(externalReference, isExternal: true);
            // RemoveName will handle all deletion logic including the nameType->nameSequence link
            RemoveName(reference);
        }
    }
}
