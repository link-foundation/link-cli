using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AcidReadinessTests
{
    [Fact]
    public void OwnedMappingsFlushWhileWriterIsOpenAndReopenNames()
    {
        var directory = Path.Combine(Path.GetTempPath(), "acid-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, "data.links");
        try
        {
            using (var data = new NamedTypesDecorator<uint>(path))
            {
                Assert.True(data.SupportsDurableFlush);
                var id = data.CreateAndUpdate(0u, 0u);
                data.SetName(id, "durable 雪");
                data.Flush();
                data.SetName(id, "second flush");
                data.Flush();
            }
            using var reopened = new NamedTypesDecorator<uint>(path);
            Assert.Equal(1u, reopened.GetByName("second flush"));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void DurableMappingFlushesAfterGrowthAndRejectsDisposedUse()
    {
        var path = Path.Combine(Path.GetTempPath(), "mapping-" + Guid.NewGuid().ToString("N"));
        var memory = new DurableMappedMemory(path, 4096);
        try
        {
            System.Runtime.InteropServices.Marshal.WriteByte(memory.Pointer, 42);
            memory.UsedCapacity = 1;
            memory.ReservedCapacity *= 2;
            Assert.Equal(42, System.Runtime.InteropServices.Marshal.ReadByte(memory.Pointer));
            System.Runtime.InteropServices.Marshal.WriteByte(memory.Pointer, 1, 43);
            memory.UsedCapacity = 2;
            memory.Flush();
            using (var reader = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
            { Assert.Equal(42, reader.ReadByte()); Assert.Equal(43, reader.ReadByte()); }
            memory.Dispose();
            Assert.Throws<ObjectDisposedException>(() => memory.Flush());
            Assert.Equal(new byte[] { 42, 43 }, File.ReadAllBytes(path));
        }
        finally { memory.Dispose(); File.Delete(path); }
    }

    [Theory]
    [InlineData("commit:bad-id")]
    [InlineData("rollback:bad-id")]
    [InlineData("applied:bad")]
    [InlineData("applied:0")]
    public void CorruptMarkersFailRecovery(string suffix)
    {
        WithStore((data, log) =>
        {
            var id = log.CreateAndUpdate(0u, 0u);
            log.SetName(id, "__transactions:" + suffix);
            Assert.Throws<InvalidDataException>(() => new TransactionsDecorator(data, log));
        });
    }

    [Fact]
    public void CompleteCorruptTransitionsFailRecovery()
    {
        var directory = Path.Combine(Path.GetTempPath(), "acid-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            var id = log.CreateAndUpdate(0u, 0u);
            log.SetName(id, "__transactions:transition:garbage");
            Assert.Throws<InvalidDataException>(() => new TransactionsDecorator(data, log));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }
    [Fact]
    public void CommitFlushesJournalThenDataThenCheckpoint()
    {
        WithStore((data, log) =>
        {
            var events = new List<string>();
            using var tx = new TransactionsDecorator(data, log,
                flushData: () => events.Add("data"), flushLog: () => events.Add("log"));
            events.Clear();
            using var transaction = tx.BeginTransaction();
            tx.CreateAndUpdate(0u, 0u);
            transaction.Commit();
            Assert.Equal(new[] { "log", "data", "log" }, events);
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
            Assert.True(tx.SupportsDurableCommit);
        });
    }

    [Fact]
    public void FailedCheckpointIsReportedWithoutAppliedMarkers()
    {
        WithStore((data, log) =>
        {
            var fail = false;
            using var tx = new TransactionsDecorator(data, log,
                flushData: () => { if (fail) throw new IOException("injected failure"); },
                flushLog: () => { });
            using var transaction = tx.BeginTransaction();
            tx.CreateAndUpdate(0u, 0u);
            fail = true;
            Assert.Throws<IOException>(() => transaction.Commit());
            Assert.True(transaction.IsCommitted);
            Assert.Equal(0, tx.AppliedSequence);
            Assert.Throws<IOException>(transaction.Dispose);
            fail = false;
            transaction.Dispose();
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
        });
    }

    private static void WithStore(Action<NamedTypesDecorator<uint>, NamedTypesDecorator<uint>> action)
    {
        var directory = Path.Combine(Path.GetTempPath(), "acid-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            action(data, log);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
