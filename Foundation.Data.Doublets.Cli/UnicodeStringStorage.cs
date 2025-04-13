using System.Numerics;
using Platform.Collections.Stacks;
using Platform.Converters;
using Platform.Data.Numbers.Raw;
using Platform.Data.Doublets;
using Platform.Data.Doublets.CriterionMatchers;
using Platform.Data.Doublets.Sequences.Converters;
using Platform.Data.Doublets.Sequences.Walkers;
using Platform.Data.Doublets.Sequences.Unicode;

namespace Foundation.Data.Doublets.Cli
{
    public class UnicodeStringStorage<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>, IComparisonOperators<TLinkAddress, TLinkAddress, bool>
    {
        private readonly ILinks<TLinkAddress> Links;
        private readonly EqualityComparer<TLinkAddress> EqualityComparer = EqualityComparer<TLinkAddress>.Default;

        private readonly AddressToRawNumberConverter<TLinkAddress> AddressToNumberConverter = new();
        private readonly RawNumberToAddressConverter<TLinkAddress> NumberToAddressConverter = new();
        private readonly BalancedVariantConverter<TLinkAddress> BalancedVariantConverter;

        public readonly TLinkAddress Type;
        public readonly TLinkAddress UnicodeSymbolType;
        public readonly TLinkAddress UnicodeSequenceType;
        public readonly TLinkAddress StringType;
        public readonly TLinkAddress EmptyStringType;
        public readonly TLinkAddress NameType;

        public IConverter<string, TLinkAddress> StringToUnicodeSequenceConverter { get; }
        public IConverter<TLinkAddress, string> UnicodeSequenceToStringConverter { get; }

        public UnicodeStringStorage(ILinks<TLinkAddress> links)
        {
            Links = links;

            (
                Type,
                UnicodeSymbolType,
                UnicodeSequenceType,
                StringType,
                EmptyStringType,
                NameType
            ) = new PinnedTypes<TLinkAddress>(links);

            BalancedVariantConverter = new BalancedVariantConverter<TLinkAddress>(links);

            var unicodeSymbolCriterionMatcher = new TargetMatcher<TLinkAddress>(links, UnicodeSymbolType);
            var unicodeSequenceCriterionMatcher = new TargetMatcher<TLinkAddress>(links, UnicodeSequenceType);

            var charToUnicodeSymbolConverter =
                new CharToUnicodeSymbolConverter<TLinkAddress>(links, AddressToNumberConverter, UnicodeSymbolType);

            var unicodeSymbolToCharConverter =
                new UnicodeSymbolToCharConverter<TLinkAddress>(links, NumberToAddressConverter, unicodeSymbolCriterionMatcher);

            StringToUnicodeSequenceConverter = new CachingConverterDecorator<string, TLinkAddress>(
                new StringToUnicodeSequenceConverter<TLinkAddress>(
                    links,
                    charToUnicodeSymbolConverter,
                    BalancedVariantConverter,
                    UnicodeSequenceType
                )
            );

            var sequenceWalker = new RightSequenceWalker<TLinkAddress>(
                links,
                new DefaultStack<TLinkAddress>(),
                unicodeSymbolCriterionMatcher.IsMatched
            );

            UnicodeSequenceToStringConverter = new CachingConverterDecorator<TLinkAddress, string>(
                new UnicodeSequenceToStringConverter<TLinkAddress>(
                    links,
                    unicodeSequenceCriterionMatcher,
                    sequenceWalker,
                    unicodeSymbolToCharConverter,
                    UnicodeSequenceType
                )
            );

            SetTypeName(Type, "Type");
            SetTypeName(UnicodeSymbolType, "UnicodeSymbol");
            SetTypeName(UnicodeSequenceType, "UnicodeSequence");
            SetTypeName(StringType, "String");
            SetTypeName(EmptyStringType, "EmptyString");
            SetTypeName(NameType, "Name");
        }

        public TLinkAddress CreateString(string content)
        {
            var stringSequence = GetStringSequence(content);
            return Links.GetOrCreate(StringType, stringSequence);
        }

        public TLinkAddress SetTypeName(TLinkAddress type, string name)
        {
            var nameSequence = CreateString(name);
            return Links.GetOrCreate(type, Links.GetOrCreate(NameType, nameSequence));
        }

        public IList<IList<TLinkAddress>?> GetTypes()
        {
            var any = Links.Constants.Any;
            var query = new Link<TLinkAddress>(any, Type, any);
            return Links.All(query);
        }

        public bool IsType(TLinkAddress address)
        {
            return Links.GetSource(address) == Type;
        }

        public TLinkAddress GetTypeByName(string name)
        {
            var nameSequence = CreateString(name);
            var nameLink = Links.SearchOrDefault(NameType, nameSequence);
            if (nameLink == Links.Constants.Null)
            {
                return Links.Constants.Null;
            }
            var any = Links.Constants.Any;
            var query = new Link<TLinkAddress>(any, any, nameLink);
            var link = Links.All(query).SingleOrDefault();
            if (link == null)
            {
                return Links.Constants.Null;
            }
            var typeCandidate = Links.GetSource(link);
            return IsType(typeCandidate)
                ? typeCandidate
                : Links.Constants.Null;
        }

        public string? GetTypeName(TLinkAddress type)
        {
            if (!IsType(type))
            {
                return null;
            }
            var any = Links.Constants.Any;
            var query = new Link<TLinkAddress>(any, type, any);
            var nameCandidatesPairs = Links.All(query);
            foreach (var nameCandidatePair in nameCandidatesPairs)
            {
                var nameCandidate = Links.GetTarget(nameCandidatePair);
                if (Links.GetSource(nameCandidate) == NameType)
                {
                    var @string = Links.GetTarget(nameCandidate);
                    return GetString(@string);
                }
            }
            return null;
        }

        public TLinkAddress GetOrCreateType(string name)
        {
            var type = GetTypeByName(name);
            if (type == Links.Constants.Null)
            {
                var @null = Links.Constants.Null;
                type = Links.CreateAndUpdate(@null, @null);
                type = Links.Update(type, Type, type);
                SetTypeName(type, name);
                return type;
            }
            return type;
        }

        private TLinkAddress GetStringSequence(string content)
            => content == ""
                ? EmptyStringType
                : StringToUnicodeSequenceConverter.Convert(content);

        public string GetString(TLinkAddress stringValue)
        {
            var current = stringValue;
            for (int i = 0; i < 3; i++)
            {
                var source = Links.GetSource(current);
                if (source == StringType)
                {
                    var sequence = Links.GetTarget(current);
                    return sequence == EmptyStringType
                        ? ""
                        : UnicodeSequenceToStringConverter.Convert(sequence);
                }
                current = Links.GetTarget(current);
            }
            throw new Exception("The passed link does not contain a string.");
        }
    }
}