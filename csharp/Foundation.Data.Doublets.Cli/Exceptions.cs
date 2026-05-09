using System;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// Exception thrown when a link has an invalid format or structure.
    /// </summary>
    public class InvalidLinkFormatException : Exception
    {
        public InvalidLinkFormatException(string message) : base(message)
        {
        }

        public InvalidLinkFormatException(string message, Exception innerException)
            : base(message, innerException)
        {
        }
    }

    /// <summary>
    /// Exception thrown when a link structure doesn't match expected patterns.
    /// </summary>
    public class UnexpectedLinkStructureException : InvalidOperationException
    {
        public UnexpectedLinkStructureException(string message) : base(message)
        {
        }

        public UnexpectedLinkStructureException(string message, Exception innerException)
            : base(message, innerException)
        {
        }
    }

    /// <summary>
    /// Exception thrown when a query pattern is invalid or cannot be processed.
    /// </summary>
    public class InvalidQueryPatternException : Exception
    {
        public InvalidQueryPatternException(string message) : base(message)
        {
        }

        public InvalidQueryPatternException(string message, Exception innerException)
            : base(message, innerException)
        {
        }
    }
}
