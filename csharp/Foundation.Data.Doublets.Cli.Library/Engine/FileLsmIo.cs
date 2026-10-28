namespace Foundation.Data.Doublets.Cli.Engine;

// Internal native boundary permits finite before/after syscall experiments.
internal class FileLsmIo
{
    internal virtual FileStream OpenOwner(string path)
    {
        var options = new FileStreamOptions { Mode = FileMode.CreateNew, Access = FileAccess.ReadWrite, Share = FileShare.Read, BufferSize = 1 };
        if (!OperatingSystem.IsWindows()) options.UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite;
        return new(path, options);
    }
    internal virtual FileStream OpenReader(string path) => new(path, FileMode.Open, FileAccess.Read, FileShare.Read, bufferSize: 1);
    internal virtual FilePageDevice OpenDevice(string path) => new(path);
    internal virtual void Close(FileStream file) => file.Dispose();
    internal virtual IEnumerable<string> Enumerate(string directory) => Directory.EnumerateFileSystemEntries(directory);
    internal virtual void Delete(string path) => File.Delete(path);
    internal virtual void SyncDirectory(string directory) => FilePageDevice.SyncDirectory(directory);
}
