namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// Constants used in query processing for pattern matching and link notation.
    /// </summary>
    public static class QueryConstants
    {
        /// <summary>
        /// Prefix for variable identifiers (e.g., "$variable").
        /// </summary>
        public const string VariablePrefix = "$";

        /// <summary>
        /// Symbol representing a wildcard match (matches any value).
        /// </summary>
        public const string WildcardSymbol = "*";

        /// <summary>
        /// Suffix for explicit link index notation (e.g., "123:").
        /// </summary>
        public const string IndexSuffix = ":";
    }
}
