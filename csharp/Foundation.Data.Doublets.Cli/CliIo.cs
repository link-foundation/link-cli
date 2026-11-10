using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data.Doublets;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// The <c>clink</c> client mode and the import, export and print helpers of
/// <c>Program.cs</c>, moved here so the entry point stays below the 900-line
/// warning threshold of <c>csharp/scripts/check-file-size.mjs</c> (issue #172).
/// </summary>
internal static class CliIo
{
    // `--connect`: sends the query to a server and prints the reply.
    // Any binary option implies `--protocol binary`.
    internal static int RunClient(string address, string? protocolName, BinaryLinoOptions binaryOptions, string query, string? token)
    {
        ILinoProtocol protocol;
        switch (protocolName?.ToLowerInvariant())
        {
            case null:
                protocol = binaryOptions == default ? new TextLinoProtocol() : new BinaryLinoProtocol(binaryOptions);
                break;
            case "text" when binaryOptions != default:
                Console.Error.WriteLine("Binary protocol options require --protocol binary.");
                return 1;
            case "text":
                protocol = new TextLinoProtocol();
                break;
            case "binary":
                protocol = new BinaryLinoProtocol(binaryOptions);
                break;
            default:
                Console.Error.WriteLine($"Invalid --protocol value '{protocolName}'. Use 'text' or 'binary'.");
                return 1;
        }
        try
        {
            using var client = LinksClient.Connect(address, protocol);
            if (token is not null)
            {
                client.Authenticate(token);
            }
            var reply = client.QueryText(query);
            if (reply.Length > 0)
            {
                Console.WriteLine(reply);
            }
            return 0;
        }
        catch (Exception error) when (error is LinoProtocolException or System.Net.Sockets.SocketException or IOException or ArgumentException)
        {
            Console.Error.WriteLine($"Error: {error.Message}");
            return 1;
        }
    }

    internal static void PrintAllLinks(INamedTypesLinks<uint> links)
    {
        LinoDatabaseOutput.WriteDatabase(links, Console.Out);
    }

    internal static void PrintChange(INamedTypesLinks<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
    {
        Console.WriteLine(LinoDatabaseOutput.FormatChange(links, linkBefore, linkAfter));
    }

    internal static bool TryWriteOutputs(INamedTypesLinks<uint> links, string? outputPath, string? binaryOutputPath)
    {
        return TryWriteLinoOutput(links, outputPath) && TryWriteBinaryOutput(links, binaryOutputPath);
    }

    internal static bool TryWriteBinaryOutput(INamedTypesLinks<uint> links, string? outputPath)
    {
        if (string.IsNullOrWhiteSpace(outputPath))
        {
            return true;
        }

        try
        {
            StoreArchive.ExportToFile(links, outputPath);
            return true;
        }
        catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException || ex is NotSupportedException || ex is LinoProtocolException)
        {
            Console.Error.WriteLine($"Error writing binary store archive '{outputPath}': {ex.Message}");
            return false;
        }
    }

    internal static bool TryReadBinaryInput(INamedTypesLinks<uint> links, string? inputPath)
    {
        if (string.IsNullOrWhiteSpace(inputPath))
        {
            return true;
        }

        try
        {
            StoreArchive.ImportFromFile(links, inputPath);
            return true;
        }
        catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException || ex is NotSupportedException || ex is LinoProtocolException || ex is InvalidOperationException)
        {
            Console.Error.WriteLine($"Error reading binary store archive '{inputPath}': {ex.Message}");
            return false;
        }
    }

    internal static bool TryWriteLinoOutput(INamedTypesLinks<uint> links, string? outputPath)
    {
        if (string.IsNullOrWhiteSpace(outputPath))
        {
            return true;
        }

        try
        {
            LinoDatabaseOutput.WriteToFile(links, outputPath);
            return true;
        }
        catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException || ex is NotSupportedException)
        {
            Console.Error.WriteLine($"Error writing LiNo output file '{outputPath}': {ex.Message}");
            return false;
        }
    }

    internal static bool TryReadLinoInput(INamedTypesLinks<uint> links, string? inputPath)
    {
        if (string.IsNullOrWhiteSpace(inputPath))
        {
            return true;
        }

        try
        {
            LinoDatabaseInput.ReadFromFile(links, inputPath);
            return true;
        }
        catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException || ex is NotSupportedException || ex is FormatException)
        {
            Console.Error.WriteLine($"Error reading LiNo input file '{inputPath}': {ex.Message}");
            return false;
        }
    }
}
