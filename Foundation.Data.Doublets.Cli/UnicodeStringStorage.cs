using System.Numerics;
using Platform.Collections.Stacks;
using Platform.Converters;
using Platform.Data.Numbers.Raw;
using Platform.Data.Doublets;
using Platform.Data.Doublets.CriterionMatchers;
using Platform.Data.Doublets.Sequences.Converters;
using Platform.Data.Doublets.Sequences.Walkers;
// using Platform.Data.Doublets.Unicode;

namespace YourNamespace
{
    public class UnicodeStringStorage<TLinkAddress>
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>, IComparisonOperators<TLinkAddress, TLinkAddress, bool>
    {
        private readonly ILinks<TLinkAddress> Links;
        private readonly EqualityComparer<TLinkAddress> EqualityComparer = EqualityComparer<TLinkAddress>.Default;

        private readonly AddressToRawNumberConverter<TLinkAddress> AddressToNumberConverter = new();
        private readonly RawNumberToAddressConverter<TLinkAddress> NumberToAddressConverter = new();
        private readonly BalancedVariantConverter<TLinkAddress> BalancedVariantConverter;

        private readonly TLinkAddress Type;
        private readonly TLinkAddress UnicodeSymbolType;
        private readonly TLinkAddress UnicodeSequenceType;

        public TLinkAddress StringType { get; }
        public TLinkAddress EmptyStringType { get; }
        public IConverter<string, TLinkAddress> StringToUnicodeSequenceConverter { get; }
        public IConverter<TLinkAddress, string> UnicodeSequenceToStringConverter { get; }

        public UnicodeStringStorage(ILinks<TLinkAddress> links)
        {
            Links = links;

            var typeAddress = TLinkAddress.One;

            Type = links.GetOrCreate(typeAddress, typeAddress++);
            UnicodeSymbolType = links.GetOrCreate(Type, typeAddress++);
            UnicodeSequenceType = links.GetOrCreate(Type, typeAddress++);
            StringType = links.GetOrCreate(Type, typeAddress++);
            EmptyStringType = links.GetOrCreate(Type, typeAddress++);

            BalancedVariantConverter = new BalancedVariantConverter<TLinkAddress>(links);

            var unicodeSymbolCriterionMatcher = new TargetMatcher<TLinkAddress>(links, UnicodeSymbolType);
            var unicodeSequenceCriterionMatcher = new TargetMatcher<TLinkAddress>(links, UnicodeSequenceType);

            // var charToUnicodeSymbolConverter =
                // new CharToUnicodeSymbolConverter<TLinkAddress>(links, AddressToNumberConverter, UnicodeSymbolType);

            // var unicodeSymbolToCharConverter =
                // new UnicodeSymbolToCharConverter<TLinkAddress>(links, NumberToAddressConverter, unicodeSymbolCriterionMatcher);

            // StringToUnicodeSequenceConverter = new CachingConverterDecorator<string, TLinkAddress>(
            //     new StringToUnicodeSequenceConverter<TLinkAddress>(
            //         links,
            //         charToUnicodeSymbolConverter,
            //         BalancedVariantConverter,
            //         UnicodeSequenceType
            //     )
            // );

            // var sequenceWalker = new RightSequenceWalker<TLinkAddress>(
            //     links,
            //     new DefaultStack<TLinkAddress>(),
            //     unicodeSymbolCriterionMatcher.IsMatched
            // );

            // UnicodeSequenceToStringConverter = new CachingConverterDecorator<TLinkAddress, string>(
            //     new UnicodeSequenceToStringConverter<TLinkAddress>(
            //         links,
            //         unicodeSequenceCriterionMatcher,
            //         sequenceWalker,
            //         unicodeSymbolToCharConverter,
            //         UnicodeSequenceType
            //     )
            // );
        }

        public TLinkAddress CreateString(string content)
        {
            var strSequence = GetStringSequence(content);
            return Links.GetOrCreate(StringType, strSequence);
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
                if (EqualityComparer.Equals(source, StringType))
                {
                    var sequence = Links.GetTarget(current);
                    return EqualityComparer.Equals(sequence, EmptyStringType)
                        ? ""
                        : UnicodeSequenceToStringConverter.Convert(sequence);
                }
                current = Links.GetTarget(current);
            }
            throw new Exception("The passed link does not contain a string.");
        }
    }
}