using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Foundation.Data.Doublets.Cli.Engine;

// Use the runtime's portable Unix status ABI rather than OS-specific stat layouts.
// Layout: dotnet/runtime v10.0.0 src/native/libs/System.Native/pal_io.h FileStatus.
internal static class NativeLsmFileType
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Status
    {
        internal int Flags, Mode;
        internal uint Uid, Gid;
        internal long Size, ATime, ATimeNsec, MTime, MTimeNsec, CTime, CTimeNsec, BirthTime, BirthTimeNsec, Dev, RDev, Ino;
        internal uint UserFlags;
    }
    [DllImport("System.Native", EntryPoint = "SystemNative_LStat", SetLastError = true)]
    private static extern int LStat([MarshalAs(UnmanagedType.LPUTF8Str)] string path, out Status status);
    [DllImport("System.Native", EntryPoint = "SystemNative_FStat", SetLastError = true)]
    private static extern int FStat(SafeFileHandle handle, out Status status);
    internal static bool Check(string path, bool directory = false, bool missing = false)
    {
        if (!OperatingSystem.IsWindows())
        {
            if (LStat(path, out var status) == 0)
            {
                if ((status.Mode & 0xf000) != (directory ? 0x4000 : 0x8000)) throw new InvalidDataException("Native LSM entry must be regular and nonsymbolic.");
                return true;
            }
            int error = Marshal.GetLastPInvokeError(); if (missing && error == 2) return false;
            throw new IOException("Cannot inspect native LSM entry.", new Win32Exception(error));
        }
        FileAttributes attributes;
        try { attributes = File.GetAttributes(path); }
        catch (FileNotFoundException) when (missing) { return false; }
        if ((attributes & (FileAttributes.ReparsePoint | FileAttributes.Device)) != 0 || attributes.HasFlag(FileAttributes.Directory) != directory)
            throw new InvalidDataException("Native LSM entry must be regular and nonsymbolic.");
        return true;
    }
    internal static void CheckHandle(FileStream file)
    {
        if (OperatingSystem.IsWindows())
        { if (!file.CanSeek) throw new InvalidDataException("Nonregular native LSM handle."); return; }
        if (FStat(file.SafeFileHandle, out var status) != 0) throw new IOException("Cannot inspect native LSM handle.", new Win32Exception(Marshal.GetLastPInvokeError()));
        if ((status.Mode & 0xf000) != 0x8000) throw new InvalidDataException("Nonregular native LSM handle.");
    }
}
