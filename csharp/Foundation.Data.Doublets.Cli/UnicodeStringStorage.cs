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

        public NamedLinks<TLinkAddress> NamedLinks { get; }

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

            NamedLinks = new NamedLinks<TLinkAddress>(
                Links,
                NameType,
                CreateString,
                GetString
            );
            NamedLinks.SetName(Type, "Type");
            NamedLinks.SetName(UnicodeSymbolType, "UnicodeSymbol");
            NamedLinks.SetName(UnicodeSequenceType, "UnicodeSequence");
            NamedLinks.SetName(StringType, "String");
            NamedLinks.SetName(EmptyStringType, "EmptyString");
            NamedLinks.SetName(NameType, "Name");
        }

        public TLinkAddress CreateString(string content)
        {
            var stringSequence = GetStringSequence(content);
            return Links.GetOrCreate(StringType, stringSequence);
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

        public TLinkAddress GetOrCreateType(string name)
        {
            var type = NamedLinks.GetByName(name);
            if (type == Links.Constants.Null)
            {
                var @null = Links.Constants.Null;
                type = Links.CreateAndUpdate(@null, @null);
                type = Links.Update(type, Type, type);
                NamedLinks.SetName(type, name);
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
            throw new InvalidLinkFormatException("The passed link does not contain a string.");
        }
    }
}