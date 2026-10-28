namespace Foundation.Data.Doublets.Cli;

/// <summary>Writes one retention archive; the live journal remains authoritative.</summary>
internal static class ChunkArchive
{
    internal static void Write(string path, IEnumerable<string> records)
    {
        // A candidate name is not ownership: another writer or an existing
        // symlink may already occupy it. Never truncate or follow that entry.
        using var stream = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.Read);
        using var writer = new StreamWriter(stream);
        foreach (var record in records) writer.WriteLine(record);
    }
}
