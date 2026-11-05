using System.Globalization;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

namespace Foundation.Data.Doublets.Cli;

/// <summary>The parts of <c>clink</c> behind <c>--permissions</c>, <c>--subject</c> and the token options (issue #149).</summary>
internal static class CliPermissions
{
    /// <summary>Reads <c>--subject</c>: a link index, or the name of a link.</summary>
    public static bool TryResolveSubject(INamedTypesLinks<uint> links, string text, out uint subject)
    {
        if (uint.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out subject))
        {
            return true;
        }
        subject = links.GetByName(text);
        return subject != links.Constants.Null;
    }

    /// <summary><c>--grant-token</c> and <c>--revoke-token</c>: writes the token table and returns the exit code.</summary>
    public static int RunTokens(LinksTokenTable table, uint subject, string? grantToken, string? revokeToken)
    {
        if (grantToken is not null)
        {
            if (subject == 0)
            {
                Console.Error.WriteLine("--grant-token needs a --subject.");
                return 1;
            }
            var link = table.Grant(grantToken, subject);
            Console.WriteLine($"Token granted to subject {subject}: {link}");
        }
        if (revokeToken is not null)
        {
            Console.WriteLine(table.Revoke(revokeToken) ? "Token revoked." : "Unknown token.");
        }
        return 0;
    }

    /// <summary>
    /// Runs a query. With <c>--permissions</c> (<paramref name="undoableLinks"/> set) it is atomic:
    /// a failed one is undone whole, even where the store changed links the subject cannot see,
    /// and <paramref name="rolledBack"/> runs before the error is rethrown.
    /// </summary>
    public static void ProcessQuery(INamedTypesLinks<uint> links, UndoableLinksDecorator<uint>? undoableLinks, QueryProcessor.Options options, Action rolledBack)
    {
        if (undoableLinks is null)
        {
            QueryProcessor.ProcessQuery(links, options);
            return;
        }
        undoableLinks.Begin();
        try
        {
            QueryProcessor.ProcessQuery(links, options);
            undoableLinks.Commit();
        }
        catch
        {
            undoableLinks.Rollback();
            rolledBack();
            throw;
        }
    }
}
