using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeCheckpointOwnershipTests
{
    private sealed class ObservedFile(string path, bool owner, bool fail, List<string> closed)
        : FileStream(path, owner ? FileMode.CreateNew : FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read, bufferSize: 1)
    {
        protected override void Dispose(bool disposing)
        {
            base.Dispose(disposing);
            if (disposing) { closed.Add(Name); if (fail) throw new IOException("cleanup " + Name); }
        }
    }
    private static string[] Names(string path, bool mirrored, bool mirroredRoot) =>
        new[] { "" }.Concat(mirroredRoot ? new[] { ".mirror" } : Array.Empty<string>())
        .Concat(mirrored ? new[] { ".bank0", ".bank0.mirror", ".bank1", ".bank1.mirror" } : new[] { ".bank0", ".bank1" }).Select(suffix => path + suffix).ToArray();
    private static IEnumerable<string> CloseOrder(IEnumerable<string> names) => names.Reverse().SelectMany(name => new[] { name + ".meta1", name + ".meta0", name, name + ".writer" });
    private static Exception[] Causes(Exception error) => error is AggregateException aggregate ? aggregate.Flatten().InnerExceptions.ToArray() : new[] { error };

    [Theory]
    [InlineData(false, false, false)]
    [InlineData(true, false, false)]
    [InlineData(false, true, false)]
    [InlineData(true, true, false)]
    [InlineData(false, false, true)]
    [InlineData(true, false, true)]
    [InlineData(false, true, true)]
    [InlineData(true, true, true)]
    public void EveryAcquisitionAndValidationFailurePreservesOriginalAndAttemptsAllRawCleanup(bool mirrored, bool failClose, bool mirroredRoot)
    {
        int count = (mirrored ? 5 : 3) + (mirroredRoot ? 1 : 0);
        for (int failed = 0; failed <= count; failed++)
        {
            string directory = Directory.CreateTempSubdirectory("checkpoint-init-").FullName, path = Path.Combine(directory, "journal");
            var opened = new List<string>(); var closed = new List<string>();
            FilePageDevice Open(string name)
            {
                if (opened.Count == failed) throw new IOException("original acquisition");
                opened.Add(name); return new(name, (file, owner) => new ObservedFile(file, owner, failClose, closed), _ => { });
            }
            try
            {
                var error = Record.Exception(() => new FileCheckpointPagedCommitLog(path, mirrored, failed == count ? 0UL : 64UL, Open, mirroredRoot));
                Assert.NotNull(error); var causes = Causes(error);
                Assert.Contains(causes, cause => cause.Message.Contains(failed == count ? "maxPages" : "original acquisition", StringComparison.Ordinal));
                Assert.Equal(CloseOrder(opened), closed);
                Assert.Equal(failClose ? 4 * failed : 0, causes.Count(cause => cause.Message.StartsWith("cleanup ", StringComparison.Ordinal)));
                foreach (string name in Names(path, mirrored, mirroredRoot)) Assert.Equal(failClose && opened.Contains(name), File.Exists(name + ".writer"));
            }
            finally { Directory.Delete(directory, true); }
        }
    }

    [Theory]
    [InlineData(false, false, false)]
    [InlineData(true, false, false)]
    [InlineData(false, true, false)]
    [InlineData(true, true, false)]
    [InlineData(false, false, true)]
    [InlineData(true, false, true)]
    [InlineData(false, true, true)]
    [InlineData(true, true, true)]
    public void ExplicitCloseAttemptsEveryDeviceRootLastAndNeverRetriesUncertainHandles(bool mirrored, bool allFail, bool mirroredRoot)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-close-").FullName, path = Path.Combine(directory, "journal");
        var closed = new List<string>(); var devices = Names(path, mirrored, mirroredRoot); int bankIndex = mirroredRoot ? 2 : 1;
        FilePageDevice Open(string name) => new(name, (file, owner) => new ObservedFile(file, owner, allFail || name == devices[bankIndex], closed), _ => { });
        try
        {
            var log = new FileCheckpointPagedCommitLog(path, mirrored, 64, Open, mirroredRoot); log.Replace(new[] { "acknowledged" });
            var error = Assert.Throws<AggregateException>(() => log.Dispose());
            Assert.Equal(CloseOrder(devices), closed); Assert.Equal(4 * (allFail ? devices.Length : 1), error.Flatten().InnerExceptions.Count);
            log.Dispose(); Assert.Equal(4 * devices.Length, closed.Count);
            Assert.Throws<ObjectDisposedException>(() => { _ = log.Epoch; }); Assert.Throws<ObjectDisposedException>(() => { log.Read(); });
            foreach (string name in devices) Assert.Equal(allFail || name == devices[bankIndex], File.Exists(name + ".writer"));
            var files = devices.SelectMany(name => new[] { name, name + ".meta0", name + ".meta1" }).ToArray(); var before = files.Select(File.ReadAllBytes).ToArray();
            Assert.Throws<IOException>(() => new FileCheckpointPagedCommitLog(path, mirrored, mirroredRoot));
            for (int index = 0; index < files.Length; index++) Assert.Equal(before[index], File.ReadAllBytes(files[index]));
            // Every injected close really released its handle before throwing.
            // Only now may this test remove those known-dead ownership files.
            foreach (string name in devices) File.Delete(name + ".writer");
            using var reopened = new FileCheckpointPagedCommitLog(path, mirrored, mirroredRoot); Assert.Equal(new[] { "acknowledged" }, reopened.Read());
        }
        finally { Directory.Delete(directory, true); }
    }
}
