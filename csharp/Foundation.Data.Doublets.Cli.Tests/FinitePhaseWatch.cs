using System.Globalization;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Tests;

// Test-process liveness only. Admit at most 64 completed, consecutive phases;
// ordinary output and partial/repeated progress cannot renew the idle bound.
internal sealed class FinitePhaseWatch : IDisposable
{
    private const string Prefix = "@link-progress:";
    private const int MaxLine = 1024;
    private readonly object gate = new();
    private readonly CancellationTokenSource deadline;
    private readonly TimeSpan idle;
    private readonly TimeProvider clock;
    private long lastProgress;
    private readonly StringBuilder line = new();
    private bool oversized;
    private int lastPhase;
    private string? failure;

    internal FinitePhaseWatch(TimeSpan idle, TimeProvider? clock = null)
    {
        this.idle = idle;
        this.clock = clock ?? TimeProvider.System;
        lastProgress = this.clock.GetTimestamp();
        deadline = new CancellationTokenSource();
    }
    // Dedicated supervision polls this clock; cancellation must not depend on
    // a timer callback being admitted to the pool whose liveness is under test.
    internal int WaitMilliseconds
    {
        get
        {
            lock (gate)
            {
                double remaining = (idle - clock.GetElapsedTime(lastProgress)).TotalMilliseconds;
                if (remaining <= 0) deadline.Cancel();
                return deadline.IsCancellationRequested ? 0 : (int)Math.Min(50, Math.Ceiling(remaining));
            }
        }
    }
    internal CancellationToken Token => deadline.Token;
    internal int LastPhase { get { lock (gate) return lastPhase; } }
    internal string? Failure { get { lock (gate) return failure; } }

    internal void Observe(ReadOnlySpan<char> text)
    {
        lock (gate)
        {
            foreach (char character in text)
            {
                if (character == '\n')
                {
                    AcceptLine();
                    line.Clear();
                    oversized = false;
                }
                else if (line.Length < MaxLine) line.Append(character);
                else oversized = true;
            }
        }
    }
    private void Reject()
    {
        failure ??= "invalid finite progress (partial, repeated, malformed or beyond 64 phases)";
        deadline.Cancel();
    }
    private void AcceptLine()
    {
        string value = line.ToString().TrimEnd('\r');
        if (!value.StartsWith(Prefix, StringComparison.Ordinal) || failure is not null) return;
        int separator = value.IndexOf(' ', Prefix.Length);
        if (oversized || separator < 0) { Reject(); return; }
        string number = value[Prefix.Length..separator];
        if (!int.TryParse(number, NumberStyles.None, CultureInfo.InvariantCulture, out int phase)
            || number != phase.ToString(CultureInfo.InvariantCulture)
            || phase != lastPhase + 1 || phase > 64
            || !value[(separator + 1)..].StartsWith("phase=", StringComparison.Ordinal)
            || value.Length <= separator + 7)
        {
            Reject(); return;
        }
        lastPhase = phase;
        lastProgress = clock.GetTimestamp();
    }
    internal void Complete()
    {
        lock (gate)
        {
            string value = line.ToString();
            if (value.Length != 0 && (value.StartsWith(Prefix, StringComparison.Ordinal)
                || Prefix.StartsWith(value, StringComparison.Ordinal))) Reject();
        }
    }
    public void Dispose() => deadline.Dispose();
}
