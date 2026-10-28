using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Foundation.Data.Doublets.Cli.Tests;

public class MappedGrowthTests
{
    [StructLayout(LayoutKind.Sequential)]
    private struct ResourceLimit { internal nuint Current, Maximum; }
    [DllImport("libc", EntryPoint = "getrlimit", SetLastError = true)]
    private static extern int GetLimit(int resource, out ResourceLimit limit);
    [DllImport("libc", EntryPoint = "setrlimit", SetLastError = true)]
    private static extern int SetLimit(int resource, ref ResourceLimit limit);

    [Fact(Timeout = 30000)]
    public async Task FailedGrowthKeepsTheCurrentMappingUsable()
    {
        if (!OperatingSystem.IsLinux()) return;
        var directory = Directory.CreateTempSubdirectory("mapping-quota-").FullName;
        try
        {
            var info = new ProcessStartInfo("bash") { RedirectStandardOutput = true, RedirectStandardError = true };
            info.Environment["DOTNET_EnableWriteXorExecute"] = "0";
            info.Environment["DOTNET_GCHeapHardLimit"] = "0x08000000";
            foreach (var argument in new[] { "-c", "trap '' XFSZ\nulimit -c 0\nulimit -S -f 8\nexec \"$@\"", "mapping-quota-child", "dotnet",
                typeof(MappedGrowthTests).Assembly.Location, "mapping-quota", directory })
                info.ArgumentList.Add(argument);
            using var child = Process.Start(info)!;
            var stdout = child.StandardOutput.ReadToEndAsync();
            var stderr = child.StandardError.ReadToEndAsync();
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            try { await child.WaitForExitAsync(deadline.Token); }
            catch (OperationCanceledException)
            {
                child.Kill(entireProcessTree: true); await child.WaitForExitAsync();
                await Task.WhenAll(stdout, stderr);
                throw new TimeoutException("Finite mapped-growth quota child did not stop.");
            }
            Assert.True(child.ExitCode == 0, $"Child exited {child.ExitCode}:\n{await stdout}\n{await stderr}");
            var bytes = File.ReadAllBytes(Path.Combine(directory, "data"));
            Assert.Equal(8193, bytes.Length);
            Assert.Equal(42, bytes[0]); Assert.Equal(99, bytes[4095]); Assert.Equal(77, bytes[8192]);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    internal static int QuotaChild(string directory)
    {
        const int fileSizeLimit = 1; // Linux RLIMIT_FSIZE, inherited only by this child.
        Assert.Equal(0, GetLimit(fileSizeLimit, out var limit));
        Assert.Equal((nuint)8192, limit.Current);
        var path = Path.Combine(directory, "data");
        try
        {
            foreach (var existing in new[] { false, true })
            {
                var initialPath = Path.Combine(directory, existing ? "existing" : "absent");
                if (existing) File.WriteAllBytes(initialPath, new byte[] { 17, 18 });
                var error = Record.Exception(() => new DurableMappedMemory(initialPath, 12288));
                Assert.True(error is IOException or ArgumentOutOfRangeException, $"Expected initialization quota rejection, got {error}");
                using (var exclusive = File.Open(initialPath, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                    Assert.Equal(existing ? 2 : 0, exclusive.Length);
                using var retry = new DurableMappedMemory(initialPath, 4096);
                Assert.Equal(existing ? 2 : 0, retry.UsedCapacity); retry.Flush();
            }
            using (var memory = new DurableMappedMemory(path, 4096))
            {
                Marshal.WriteByte(memory.Pointer, 41); Marshal.WriteByte(memory.Pointer, 4095, 99);
                memory.UsedCapacity = 4096; memory.Flush();
                Marshal.WriteByte(memory.Pointer, 42); // Unflushed changes also survive rejection.
                var originalPointer = memory.Pointer;
                for (var attempt = 0; attempt < 2; ++attempt)
                {
                    var error = Record.Exception(() => memory.ReservedCapacity = 12288);
                    Assert.True(error is IOException or ArgumentOutOfRangeException, $"Expected quota rejection, got {error}");
                    Console.Error.WriteLine($"Expected bounded growth rejection: {error.Message}");
                    Assert.Equal(4096, memory.ReservedCapacity); Assert.Equal(4096, memory.UsedCapacity);
                    Assert.Equal(originalPointer, memory.Pointer);
                    // This must remain usable before any attempt to read an unsafe pointer.
                    memory.Flush();
                    Assert.Equal(42, Marshal.ReadByte(memory.Pointer));
                    Assert.Equal(99, Marshal.ReadByte(memory.Pointer, 4095));
                    Assert.Throws<IOException>(() => File.Open(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None));
                }
                limit.Current = limit.Maximum;
                Assert.Equal(0, SetLimit(fileSizeLimit, ref limit));
                memory.ReservedCapacity = 12288;
                Assert.Equal(42, Marshal.ReadByte(memory.Pointer));
                Assert.Equal(99, Marshal.ReadByte(memory.Pointer, 4095));
                Marshal.WriteByte(memory.Pointer, 8192, 77); memory.UsedCapacity = 8193; memory.Flush();
            }
            using var reopened = new DurableMappedMemory(path, 4096);
            Assert.Equal(8193, reopened.UsedCapacity);
            Assert.Equal(42, Marshal.ReadByte(reopened.Pointer));
            Assert.Equal(99, Marshal.ReadByte(reopened.Pointer, 4095));
            Assert.Equal(77, Marshal.ReadByte(reopened.Pointer, 8192));
            reopened.Flush();
            return 0;
        }
        finally
        {
            // Restore the child's quota for the coverage collector's hits file.
            limit.Current = limit.Maximum;
            Assert.Equal(0, SetLimit(fileSizeLimit, ref limit));
        }
    }

    [Fact]
    public void ExistingBytesSurviveRepeatedGrowthAndRejectedCapacities()
    {
        var path = Path.Combine(Path.GetTempPath(), "mapping-growth-" + Guid.NewGuid().ToString("N"));
        var expected = Enumerable.Range(0, 4096).Select(value => (byte)(value % 251)).ToArray();
        File.WriteAllBytes(path, expected);
        DurableMappedMemory? memory = null;
        try
        {
            memory = new DurableMappedMemory(path, 4096);
            Assert.Equal(4096, memory.UsedCapacity);
            Assert.True(memory.ReservedCapacity >= memory.UsedCapacity);
            foreach (var capacity in new[] { 12288L, 4096L, 16384L })
            {
                memory.ReservedCapacity = capacity;
                var copy = new byte[expected.Length]; Marshal.Copy(memory.Pointer, copy, 0, copy.Length);
                Assert.Equal(expected, copy);
                var pointer = memory.Pointer;
                Assert.Throws<ArgumentOutOfRangeException>(() => memory.ReservedCapacity = expected.Length - 1);
                Assert.Equal(capacity, memory.ReservedCapacity); Assert.Equal(pointer, memory.Pointer);
                memory.Flush();
                Assert.Throws<IOException>(() => File.Open(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None));
                using var reader = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
                var actual = new byte[expected.Length]; reader.ReadExactly(actual); Assert.Equal(expected, actual);
            }
            memory.Dispose(); memory.Dispose();
            Assert.Throws<ObjectDisposedException>(() => memory.Flush());
            Assert.Throws<ObjectDisposedException>(() => memory.ReservedCapacity = 8192);
            Assert.Equal(expected, File.ReadAllBytes(path));
        }
        finally { memory?.Dispose(); File.Delete(path); }
    }
}
