using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;
public class NativeLsmBeforeTests
{
    [Fact]
    public void NativeLsmOwnsFilesAndReopensAcknowledgedJournals()
    {
        string directory = Path.Combine(Path.GetTempPath(), "native-lsm-before-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            string[] keys = { "a", "b" }; var options = new FileLsmOptions(MaxPages: 128);
            using (var store = FileLsmJournalStore.Open(directory, options))
            {
                Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "durable\0雪😀" }, Array.Empty<string>() }));
                store.Flush(); Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, options));
            }
            using (var store = FileLsmJournalStore.Open(directory, options))
            { var values = store.Read(keys); Assert.Equal(new[] { "durable\0雪😀" }, values[0].Entries); Assert.Empty(values[1].Entries); }
            Assert.DoesNotContain(Directory.EnumerateFiles(directory), path => path.EndsWith(".writer", StringComparison.Ordinal) || Path.GetFileName(path) == ".lsm-writer");
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
