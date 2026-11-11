using System.Runtime.Versioning;

namespace Foundation.Data.Doublets.Cli.Ipc;

[SupportedOSPlatform("windows")]
internal static class WindowsPipe
{
    public static IpcStream Connect(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
    public static IIpcListener Listen(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
}
[SupportedOSPlatform("windows")]
internal static class WindowsMailslot
{
    public static IpcStream Connect(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
    public static IIpcListener Listen(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
}
[SupportedOSPlatform("windows")]
internal static class WindowsCopyData
{
    public static IpcStream Connect(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
    public static IIpcListener Listen(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
}
[SupportedOSPlatform("windows")]
internal static class WindowsAlpc
{
    public static IpcStream Connect(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
    public static IIpcListener Listen(Endpoint endpoint) => throw IpcException.Unsupported(endpoint.Kind, "this");
}
