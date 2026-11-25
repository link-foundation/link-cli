using System.Globalization;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using Foundation.Data.Doublets.Cli.Ipc;

namespace Foundation.Data.Doublets.Cli;

/// <summary>Whether the owner of a store lock still runs, as far as this host can tell.</summary>
public enum StoreLockLivenessState
{
    /// <summary>The owner runs.</summary>
    Alive,
    /// <summary>The owner is gone; its lock may be broken.</summary>
    Gone,
    /// <summary>Another host or PID namespace: nothing here can tell.</summary>
    Unknown,
}

/// <summary>The liveness of a lock owner and, unless it runs, how that is known.</summary>
public readonly record struct StoreLockLiveness(StoreLockLivenessState State, string Reason);

/// <summary>
/// Who holds a store lock (issue #194). The record is a few <c>key=value</c>
/// lines, the same in the C#, Rust and JavaScript <c>clink</c>:
/// <code>
/// pid=4242
/// host=build-7
/// pid_namespace=pid:[4026531836]
/// start_time=1234567
/// acquired_at_ms=1764028800000
/// token=4242.5f0c2a9d1e3b4c6a
/// program=clink (csharp) 5.2.1
/// </code>
/// <c>pid_namespace</c> and <c>start_time</c> are only known on Linux;
/// elsewhere they are empty and the owner is judged by host and process id.
/// </summary>
public sealed record StoreLockOwner
{
    public uint Pid { get; init; }
    public string Host { get; init; } = "";
    public string PidNamespace { get; init; } = "";
    public string StartTime { get; init; } = "";
    public ulong AcquiredAtMs { get; init; }
    public string Token { get; init; } = "";
    public string Program { get; init; } = "";

    /// <summary>The record this process writes when it takes a lock.</summary>
    public static StoreLockOwner Current(string token)
    {
        var pid = (uint)Environment.ProcessId;
        return new StoreLockOwner
        {
            Pid = pid,
            Host = HostName(),
            PidNamespace = CurrentPidNamespace(),
            StartTime = ProcessStartTime(pid) ?? "",
            AcquiredAtMs = (ulong)DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            Token = token,
            Program = ProgramName,
        };
    }

    /// <summary>The <c>key=value</c> lines of the record.</summary>
    public string ToRecord()
    {
        return string.Create(
          CultureInfo.InvariantCulture,
          $"pid={Pid}\nhost={Host}\npid_namespace={PidNamespace}\nstart_time={StartTime}\nacquired_at_ms={AcquiredAtMs}\ntoken={Token}\nprogram={Program}\n");
    }

    /// <summary>
    /// Reads a record; <see langword="null"/> unless it names a process id.
    /// Unknown keys are ignored so that later versions can add fields.
    /// </summary>
    public static StoreLockOwner? Parse(string? text)
    {
        if (text is null) return null;
        var owner = new StoreLockOwner();
        foreach (var line in text.Split('\n'))
        {
            var separator = line.IndexOf('=', StringComparison.Ordinal);
            if (separator < 0) continue;
            var value = line[(separator + 1)..].Trim();
            switch (line[..separator].Trim())
            {
                case "pid":
                    if (!uint.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out var pid)) return null;
                    owner = owner with { Pid = pid };
                    break;
                case "host": owner = owner with { Host = value }; break;
                case "pid_namespace": owner = owner with { PidNamespace = value }; break;
                case "start_time": owner = owner with { StartTime = value }; break;
                case "acquired_at_ms":
                    owner = owner with
                    {
                        AcquiredAtMs = ulong.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out var at) ? at : 0
                    };
                    break;
                case "token": owner = owner with { Token = value }; break;
                case "program": owner = owner with { Program = value }; break;
            }
        }
        return owner.Pid != 0 ? owner : null;
    }

    /// <summary>One line for messages: every known field, then whether it still runs.</summary>
    public string Describe()
    {
        var text = new StringBuilder(string.Create(CultureInfo.InvariantCulture, $"pid={Pid} host={Host}"));
        foreach (var (key, value) in new[] { ("pid_namespace", PidNamespace), ("start_time", StartTime), ("program", Program) })
        {
            if (value.Length > 0) text.Append(CultureInfo.InvariantCulture, $" {key}={value}");
        }
        if (AcquiredAtMs != 0) text.Append(CultureInfo.InvariantCulture, $" acquired_at_ms={AcquiredAtMs}");
        var liveness = Liveness();
        text.Append(liveness.State == StoreLockLivenessState.Alive ? " (running)" : $" ({liveness.Reason})");
        return text.ToString();
    }

    /// <summary>
    /// Whether the owner still runs. Only an owner on this host and in this
    /// PID namespace can be judged; a reused process id is told apart by its
    /// start time where the system reports one.
    /// </summary>
    public StoreLockLiveness Liveness() => Liveness(IpcTransports.ProcessAlive);

    /// <summary><see cref="Liveness()"/> with the process probe given.</summary>
    public StoreLockLiveness Liveness(Func<uint, bool> processAlive)
    {
        ArgumentNullException.ThrowIfNull(processAlive);
        if (Host != HostName())
        {
            return new(StoreLockLivenessState.Unknown, $"held on another host, {Host}");
        }
        if (PidNamespace != CurrentPidNamespace())
        {
            return new(StoreLockLivenessState.Unknown, "held in another PID namespace");
        }
        if (!processAlive(Pid))
        {
            return new(StoreLockLivenessState.Gone, string.Create(CultureInfo.InvariantCulture, $"owner process {Pid} is gone"));
        }
        if (StartTime.Length > 0 && ProcessStartTime(Pid) is { } started && started != StartTime)
        {
            return new(
              StoreLockLivenessState.Gone,
              string.Create(CultureInfo.InvariantCulture, $"owner process {Pid} is gone; its id now belongs to another process"));
        }
        return new(StoreLockLivenessState.Alive, "");
    }

    /// <summary>
    /// <c>&lt;pid&gt;.&lt;random&gt;</c>: a token no other process, nor
    /// another call in this one, is likely to share.
    /// </summary>
    public static string UniqueToken() =>
        string.Create(CultureInfo.InvariantCulture, $"{Environment.ProcessId}.{Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(8))}");

    /// <summary>
    /// The name of this host as the Rust and JavaScript <c>clink</c> read it:
    /// <c>gethostname</c> on Unix, <c>COMPUTERNAME</c> on Windows.
    /// </summary>
    public static string HostName()
    {
        if (OperatingSystem.IsWindows())
        {
            return Environment.GetEnvironmentVariable("COMPUTERNAME") ?? "";
        }
        try
        {
            // Unlike Environment.MachineName, the whole name, dots included.
            return System.Net.Dns.GetHostName();
        }
        catch (System.Net.Sockets.SocketException)
        {
            return "";
        }
    }

    /// <summary>
    /// The PID namespace of this process (Linux), so that a container does not
    /// judge the process ids of another; empty elsewhere.
    /// </summary>
    public static string CurrentPidNamespace()
    {
        if (!OperatingSystem.IsLinux()) return "";
        try
        {
            return new FileInfo("/proc/self/ns/pid").LinkTarget ?? "";
        }
        catch (IOException)
        {
            return "";
        }
        catch (UnauthorizedAccessException)
        {
            return "";
        }
    }

    /// <summary>
    /// When <paramref name="pid"/> started, in clock ticks since boot (Linux);
    /// <see langword="null"/> elsewhere or when the process does not exist.
    /// </summary>
    public static string? ProcessStartTime(uint pid)
    {
        if (!OperatingSystem.IsLinux()) return null;
        string stat;
        try
        {
            stat = File.ReadAllText(string.Create(CultureInfo.InvariantCulture, $"/proc/{pid}/stat"));
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }
        return StartTimeOfStat(stat);
    }

    /// <summary>
    /// The <c>starttime</c> of a <c>/proc/&lt;pid&gt;/stat</c> line. The
    /// command name in parentheses may hold spaces; the fields after it start
    /// with the state (field 3), so <c>starttime</c> (field 22) is the 20th.
    /// </summary>
    public static string? StartTimeOfStat(string stat)
    {
        ArgumentNullException.ThrowIfNull(stat);
        var close = stat.LastIndexOf(')');
        if (close < 0) return null;
        var fields = stat[(close + 1)..].Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
        return fields.Length > 19 ? fields[19] : null;
    }

    private static readonly string ProgramName = ReadProgramName();

    private static string ReadProgramName()
    {
        var assembly = Assembly.GetEntryAssembly() ?? typeof(StoreLockOwner).Assembly;
        var version = assembly.GetName().Version;
        return version is null
          ? "clink (csharp)"
          : string.Create(CultureInfo.InvariantCulture, $"clink (csharp) {version.ToString(3)}");
    }
}
