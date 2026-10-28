using Platform.Data;
using Platform.Data.Doublets;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli.Tests;

public class ConstructorFailureTests
{
    private static IDisposable Open(string kind, string path) => kind switch
    {
        "typed" => new NamedTypesDecorator<uint>(path),
        "named" => new NamedLinksDecorator<uint>(path),
        "simple" => new SimpleLinksDecorator<uint>(path),
        _ => throw new ArgumentException(nameof(kind))
    };

    [Theory]
    [InlineData("typed")]
    [InlineData("named")]
    [InlineData("simple")]
    public void FailedOwnedNamesConstructionReleasesTheDataHandle(string kind)
    {
        var directory = Path.Combine(Path.GetTempPath(), "constructor-failure-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, "data.links");
        var namesPath = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(path);
        Directory.CreateDirectory(namesPath);
        try
        {
            var error = Record.Exception(() => { using var attempt = Open(kind, path); });
            Assert.True(error is IOException or UnauthorizedAccessException, $"Expected blocked names path, got {error}");
            // A failed constructor owns no live store. Immediate exclusive
            // reopening checks release without waiting for GC/finalizers.
            using (var exclusive = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                Assert.True(exclusive.CanWrite);
            Directory.Delete(namesPath);
            using var reopened = Open(kind, path);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData("typed")]
    [InlineData("named")]
    [InlineData("simple")]
    [InlineData("pinned")]
    public void FailedBorrowedNamesConstructionLeavesTheCallerStoreUsable(string kind)
    {
        var directory = Path.Combine(Path.GetTempPath(), "borrowed-constructor-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            var blocked = Path.Combine(directory, "blocked"); Directory.CreateDirectory(blocked);
            var error = Record.Exception(() =>
            {
                using IDisposable attempt = kind switch
                {
                    "typed" => new NamedTypesDecorator<uint>(data, blocked),
                    "named" => new NamedLinksDecorator<uint>(data, blocked),
                    "simple" => new SimpleLinksDecorator<uint>(data, blocked),
                    "pinned" => new NamedTypesDecorator<uint>(new PinnedTypesDecorator<uint>(data), blocked),
                    _ => throw new ArgumentException(nameof(kind))
                };
            });
            Assert.True(error is IOException or UnauthorizedAccessException, $"Expected blocked names path, got {error}");
            var id = data.CreateAndUpdate(0u, 0u);
            data.Flush();
            Assert.True(data.Exists(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private sealed class UnavailableMemory : IResizableDirectMemory
    {
        public bool Disposed { get; private set; }
        private static IOException Unavailable() => new("injected memory access");
        public IntPtr Pointer => throw Unavailable();
        public long Size => throw Unavailable();
        public long ReservedCapacity { get => throw Unavailable(); set => throw Unavailable(); }
        public long UsedCapacity { get => throw Unavailable(); set => throw Unavailable(); }
        public void Dispose() => Disposed = true;
    }

    [Fact]
    public void FailedNamesFacadeInitializationDisposesItsMapping()
    {
        using var memory = new UnavailableMemory();
        var error = Assert.Throws<IOException>(() => NamesDatabase<uint>.Open(memory));
        Assert.Equal("injected memory access", error.Message);
        Assert.True(memory.Disposed);
    }

    private sealed class QuotaMemory : IResizableDirectMemory
    {
        private readonly HeapResizableDirectMemory _inner = new();
        public bool Disposed { get; private set; }
        public IntPtr Pointer => _inner.Pointer;
        public long Size => _inner.Size;
        public long ReservedCapacity { get => _inner.ReservedCapacity; set => _inner.ReservedCapacity = value; }
        public long UsedCapacity
        {
            get => _inner.UsedCapacity;
            set
            {
                // The raw uint store's header fits; names initialization must
                // allocate additional links and exhaust this finite quota.
                if (value > 128) throw new IOException("injected names quota");
                _inner.UsedCapacity = value;
            }
        }
        public void Dispose() { Disposed = true; _inner.Dispose(); }
    }

    [Fact]
    public void FailedNamesContentInitializationDisposesTheFacadeAndMapping()
    {
        using var memory = new QuotaMemory();
        var error = Assert.Throws<IOException>(() => NamesDatabase<uint>.Open(memory));
        Assert.Equal("injected names quota", error.Message);
        Assert.Contains(nameof(UnicodeStringStorage<uint>), error.StackTrace);
        Assert.True(memory.Disposed);
    }
}
