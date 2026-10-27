using System.Collections;
using System.Runtime.InteropServices;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeLsmFileOwnershipTests
{
    private static FileLsmOptions Options => new(PhysicalLsmHarness.Options(), 128, true, true, true);
    private static void Seed(string directory)
    { using var store = FileLsmJournalStore.Open(directory, Options); Assert.True(store.CompareExchange(new[] { "a" }, store.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "saved" } })); store.Flush(); }
    private static void Run(Action<string> action)
    { string directory = Directory.CreateTempSubdirectory("lsm-ownership-").FullName; try { action(directory); } finally { Directory.Delete(directory, true); } }
    private static IEnumerable<Exception> Causes(Exception error) => error is AggregateException aggregate ? aggregate.Flatten().InnerExceptions : new[] { error };
    [Fact(Timeout = 120000)]
    public Task UncertainChildAndOuterClosesAreCachedWithoutRetryAndAllOthersAreAttempted() => Task.Run(() => Run(directory =>
    {
        var io = new NativeLsmFileHarness(); Seed(directory);
        using (var probe = FileLsmJournalStore.Open(directory, Options, io)) { io.Reset(); probe.Dispose(); }
        string[] trace = io.Trace.ToArray(); io.Active = false;
        int[] points = new[] { 1, Array.FindIndex(trace, value => value == "close:wal.meta0") + 1, Array.FindIndex(trace, value => value == "close:.lsm-writer") + 1, trace.Length };
        foreach (int at in points.Distinct()) foreach (bool after in new[] { false, true })
            {
                Assert.InRange(at, 1, trace.Length);
                try
                {
                    // The holder retains cleanup without masking an earlier assertion.
                    using (var owner = new NativeLsmFileHarness.Owner(directory, Options, io))
                    {
                        var store = owner.Store; io.Reset((at, after)); var first = Assert.ThrowsAny<Exception>(store.Dispose); Assert.Contains(Causes(first), error => error.Message.Contains("native LSM operation cut", StringComparison.Ordinal));
                        Assert.True(io.Hit); int calls = io.Trace.Count; io.Active = false;
                        Assert.Throws<InvalidOperationException>(store.Dispose); Assert.Equal(calls, io.Trace.Count);
                        Assert.Throws<ObjectDisposedException>(() => store.Read(new[] { "a" }));
                        Assert.Equal(!after && trace[at - 1].StartsWith("close:", StringComparison.Ordinal) ? 1 : 0, io.Live.Count);
                        if (at < trace.Length || !after) Assert.True(File.Exists(Path.Combine(directory, ".lsm-writer")));
                    }
                }
                finally { io.Recover(directory); }
                using var reopened = FileLsmJournalStore.Open(directory, Options); Assert.Equal("saved", reopened.Read(new[] { "a" })[0].Entries[0]);
            }
    }));
    private sealed class BrokenEnumeration(bool duplicate, bool closeFailure) : FileLsmIo
    {
        internal int Reads;
        internal override IEnumerable<string> Enumerate(string directory) => new Entries(this, directory, duplicate, closeFailure);
        private sealed class Entries(BrokenEnumeration owner, string directory, bool duplicate, bool closeFailure) : IEnumerable<string>, IEnumerator<string>
        {
            public string Current => Path.Combine(directory, ".lsm-writer");
            object IEnumerator.Current => Current;
            public bool MoveNext() { owner.Reads++; if (!duplicate) throw new IOException("enumeration primary"); return true; }
            public void Reset() => throw new NotSupportedException();
            public void Dispose() { if (closeFailure) throw new IOException("enumeration cleanup"); }
            public IEnumerator<string> GetEnumerator() => this;
            IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        }
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void StreamingEnumerationBoundsBeforeCollectionAndKeepsEveryFailure(bool duplicate) => Run(directory =>
    {
        var io = new BrokenEnumeration(duplicate, closeFailure: true);
        var error = Assert.Throws<AggregateException>(() => FileLsmJournalStore.Open(directory, Options, io));
        Assert.Contains(Causes(error), cause => cause.Message.Contains(duplicate ? "bound" : "enumeration primary", StringComparison.Ordinal));
        Assert.Contains(Causes(error), cause => cause.Message.Contains("enumeration cleanup", StringComparison.Ordinal)); Assert.InRange(io.Reads, 1, 200);
        Assert.True(File.Exists(Path.Combine(directory, ".lsm-writer")));
    });
    [DllImport("libc", EntryPoint = "mkfifo", SetLastError = true)]
    private static extern int MakeFifo([MarshalAs(UnmanagedType.LPUTF8Str)] string path, uint mode);
    [Fact]
    public void SymlinksDirectoriesAndUnixSpecialFilesRejectBeforeAnyBlockingOpen() => Run(directory =>
    {
        string target = Path.Combine(directory, "target"); File.WriteAllText(target, "evidence");
        string symbolic = Path.Combine(directory, "symbolic");
        if (!OperatingSystem.IsWindows())
        {
            File.CreateSymbolicLink(symbolic, target); Assert.Throws<InvalidDataException>(() => NativeLsmFileType.Check(symbolic));
            File.Delete(target); Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options)); File.Delete(symbolic);
            string fifo = Path.Combine(directory, "wal"); Assert.Equal(0, MakeFifo(fifo, 0x180));
            Assert.Throws<InvalidDataException>(() => NativeLsmFileType.Check(fifo));
            Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options)); File.Delete(fifo);
            string parent = directory + "-alias"; Directory.CreateSymbolicLink(parent, directory);
            try { Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(parent, Options)); } finally { Directory.Delete(parent); }
        }
        Directory.CreateDirectory(Path.Combine(directory, "foreign")); Assert.Throws<InvalidDataException>(() => NativeLsmFileType.Check(Path.Combine(directory, "foreign")));
        Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options)); File.WriteAllText(target, "evidence"); Assert.Equal("evidence", File.ReadAllText(target));
    });
}
