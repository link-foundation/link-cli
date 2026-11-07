using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

// Finite faults at actual FileStream/native namespace operation boundaries.
internal sealed class NativeLsmFileHarness : FileLsmIo
{
    internal readonly List<string> Trace = new();
    internal readonly HashSet<ObservedFile> Live = new();
    internal (int At, bool After)? Fault;
    internal bool Active, Hit;
    internal T Effect<T>(string operation, Func<T> action)
    {
        if (!Active) return action(); Trace.Add(operation); bool hit = Fault?.At == Trace.Count; if (hit) Hit = true;
        if (hit && !Fault!.Value.After) throw new IOException("native LSM operation cut");
        T result = action(); if (hit) throw new IOException("native LSM operation cut"); return result;
    }
    internal void Effect(string operation, Action action) => Effect(operation, () => { action(); return true; });
    private ObservedFile Open(string path, bool owner, bool reader = false) => Effect((reader ? "read-open:" : "create:") + Path.GetFileName(path), () =>
    { var file = new ObservedFile(this, path, owner, reader); Live.Add(file); return file; });
    internal override FileStream OpenOwner(string path) => Open(path, true);
    internal override FileStream OpenReader(string path) => Open(path, false, true);
    internal override FilePageDevice OpenDevice(string path) => new(path, (name, owner) => Open(name, owner), directory => Effect("directory-sync:create", () => base.SyncDirectory(directory)));
    internal override void Delete(string path) => Effect("delete:" + Path.GetFileName(path), () => base.Delete(path));
    internal override void SyncDirectory(string directory) => Effect("directory-sync:delete", () => base.SyncDirectory(directory));
    internal void Reset((int, bool)? fault = null) { Trace.Clear(); Fault = fault; Hit = false; Active = true; }
    internal void Recover(string directory)
    {
        Active = false; foreach (var file in Live.ToArray()) file.ForceClose(); Assert.Empty(Live);
        // Explicit external recovery, only after every test handle is closed.
        foreach (string path in Directory.GetFiles(directory)) if (Path.GetFileName(path) == ".lsm-writer" || path.EndsWith(".writer", StringComparison.Ordinal)) File.Delete(path);
    }
    // Own the facade through injected close failures without replacing a test's
    // assertion with a second cached cleanup exception. External recovery follows.
    internal sealed class Owner : IDisposable
    {
        internal readonly FileLsmJournalStore Store;
        internal Exception? Cleanup;
        internal Owner(string directory, FileLsmOptions options, NativeLsmFileHarness io)
        { Store = FileLsmJournalStore.Open(directory, options, io); }
        public void Dispose() { try { Store.Dispose(); } catch (Exception error) { Cleanup = error; } }
    }
    internal sealed class ObservedFile(NativeLsmFileHarness owner, string path, bool marker, bool reader)
        : FileStream(path, reader ? FileMode.Open : marker ? FileMode.CreateNew : FileMode.OpenOrCreate, reader ? FileAccess.Read : FileAccess.ReadWrite, FileShare.Read, bufferSize: 1)
    {
        public override void Write(ReadOnlySpan<byte> bytes)
        {
            // A span cannot be captured; the bounded page/metadata copy is test-only.
            byte[] copy = bytes.ToArray(); owner.Effect("write:" + Path.GetFileName(Name), () => base.Write(copy.AsSpan()));
        }
        public override void SetLength(long length) => owner.Effect("truncate:" + Path.GetFileName(Name), () => base.SetLength(length));
        public override void Flush(bool flushToDisk) => owner.Effect("sync:" + Path.GetFileName(Name), () => base.Flush(flushToDisk));
        protected override void Dispose(bool disposing)
        {
            if (!disposing) { base.Dispose(false); return; }
            owner.Effect("close:" + Path.GetFileName(Name), () => { base.Dispose(true); owner.Live.Remove(this); });
        }
        internal void ForceClose() { base.Dispose(true); owner.Live.Remove(this); }
    }
}
