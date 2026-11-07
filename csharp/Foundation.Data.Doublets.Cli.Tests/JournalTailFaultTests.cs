using System.Text;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class JournalTailFaultTests
{
    private sealed class FaultFile : FileStream
    {
        internal int? PartialBytes;
        internal bool FailRead, FailTruncate, FailFlush, FailAfterFlush;
        internal FaultFile(string path) : base(path, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read, bufferSize: 1) { }
        public override void Write(ReadOnlySpan<byte> bytes)
        {
            if (PartialBytes is { } count)
            {
                base.Write(bytes[..Math.Min(count, bytes.Length)]);
                throw new IOException("injected write after effect");
            }
            base.Write(bytes);
        }
        public override int Read(Span<byte> bytes)
        {
            if (FailRead) throw new IOException("injected repair read");
            return base.Read(bytes);
        }
        public override void SetLength(long value)
        {
            if (FailTruncate) throw new IOException("injected repair truncate");
            base.SetLength(value);
        }
        public override void Flush(bool flushToDisk)
        {
            if (FailFlush) throw new IOException("injected repair sync");
            base.Flush(flushToDisk);
            if (FailAfterFlush) throw new IOException("injected flush after effect");
        }
    }

    private static void WithLog(Action<FileCommitLog, FaultFile> run)
    {
        var directory = Directory.CreateTempSubdirectory("journal-tail-fault-").FullName;
        var path = Path.Combine(directory, "journal");
        try
        {
            FaultFile? file = null;
            using (var log = new FileCommitLog(path, name => file = new FaultFile(name)))
            {
                log.Append("committed"); log.Flush();
                run(log, file!);
            }
            using var reopened = new FileCommitLog(path);
            Assert.Equal(new[] { "committed", "following" }, reopened.Read());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData("read", false)]
    [InlineData("read", true)]
    [InlineData("append", false)]
    [InlineData("append", true)]
    [InlineData("flush", false)]
    [InlineData("flush", true)]
    public void PartialWriteRepairsBeforeTheNextOperation(string operation, bool unicode)
    {
        WithLog((log, file) =>
        {
            file.PartialBytes = 1;
            Assert.Throws<IOException>(() => log.Append(unicode ? "雪" : "uncommitted"));
            Assert.Equal(11, file.Length);
            file.PartialBytes = null;
            if (operation == "read") Assert.Equal(new[] { "committed" }, log.Read());
            if (operation == "flush") { log.Flush(); Assert.Equal(new[] { "committed" }, log.Read()); }
            log.Append("following"); log.Flush();
            Assert.Equal(new[] { "committed", "following" }, log.Read());
        });
    }

    [Theory]
    [InlineData("read")]
    [InlineData("truncate")]
    [InlineData("sync")]
    public void RepairFailureRetainsAdmissionUntilSuccessfulRetry(string phase)
    {
        WithLog((log, file) =>
        {
            file.PartialBytes = 10000;
            Assert.Throws<IOException>(() => log.Append(new string('x', 20000)));
            file.PartialBytes = null;
            file.FailRead = phase == "read"; file.FailTruncate = phase == "truncate"; file.FailFlush = phase == "sync";
            Assert.Throws<IOException>(() => log.Read());
            Assert.Throws<IOException>(() => log.Append("blocked"));
            Assert.Throws<IOException>(() => log.Flush());
            file.FailRead = file.FailTruncate = file.FailFlush = false;
            log.Append("following"); log.Flush();
        });
    }

    [Theory]
    [InlineData("append")]
    [InlineData("flush")]
    public void CompleteRecordIsKeptAfterAnErrorFollowingItsEffect(string phase)
    {
        WithLog((log, file) =>
        {
            if (phase == "append")
            {
                file.PartialBytes = int.MaxValue;
                Assert.Throws<IOException>(() => log.Append("following"));
                file.PartialBytes = null;
            }
            else
            {
                log.Append("following"); file.FailAfterFlush = true;
                Assert.Throws<IOException>(() => log.Flush());
                file.FailAfterFlush = false;
            }
            Assert.Equal(new[] { "committed", "following" }, log.Read());
            log.Flush();
        });
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void ConstructionFailureReleasesTheFileAndWriterFence(bool duringRepair)
    {
        var directory = Directory.CreateTempSubdirectory("journal-open-fault-").FullName;
        var path = Path.Combine(directory, "journal");
        try
        {
            Assert.Throws<IOException>(() => new FileCommitLog(path, file => duringRepair
                ? new FaultFile(file) { FailFlush = true } : throw new IOException("injected open")));
            using var recovered = new FileCommitLog(path);
            Assert.Empty(recovered.Read());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void CompleteInvalidUtf8RemainsVisibleAsCorruption()
    {
        var directory = Directory.CreateTempSubdirectory("journal-bad-utf8-").FullName;
        var path = Path.Combine(directory, "journal");
        byte[] bytes = [255, 10];
        try
        {
            File.WriteAllBytes(path, bytes);
            using (var log = new FileCommitLog(path)) Assert.Throws<DecoderFallbackException>(() => log.Read());
            Assert.Equal(bytes, File.ReadAllBytes(path));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
