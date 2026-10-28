namespace Foundation.Data.Doublets.Cli;

/// <summary>The original uint query API, sharing the selected-address implementation.</summary>
public static class AdvancedMixedQueryProcessor
{
    public class Options : AdvancedMixedQueryProcessor<uint>.Options
    {
        public static implicit operator Options(string query) => new() { Query = query };
    }

    public static void ProcessQuery(INamedTypesLinks<uint> links, Options options) =>
        AdvancedMixedQueryProcessor<uint>.ProcessQuery(links, options);

    public class Pattern
    {
        public string Index;
        public Pattern? Source;
        public Pattern? Target;

        public Pattern(string index, Pattern? source = null, Pattern? target = null)
        {
            Index = index ?? "";
            Source = source;
            Target = target;
        }

        public bool IsLeaf => Source == null && Target == null;
    }
}
