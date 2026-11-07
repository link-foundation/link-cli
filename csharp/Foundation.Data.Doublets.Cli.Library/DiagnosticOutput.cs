namespace Foundation.Data.Doublets.Cli;

internal static class DiagnosticOutput
{
    // A diagnostic destination must not change a storage operation's outcome.
    internal static void WriteLine(string message)
    { try { Console.WriteLine(message); } catch { /* Preserve the storage result. */ } }

    internal static void WriteErrorLine(string message)
    { try { Console.Error.WriteLine(message); } catch { /* Preserve the storage result. */ } }
}
