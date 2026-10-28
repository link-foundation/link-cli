using System.Collections.Concurrent;

namespace Foundation.Data.Doublets.Cli.Tests;

public class FiniteExitContextTests
{
    [Fact(Timeout = 15000)]
    public async Task DeferredCallerContextCannotDelayFiniteProgressSupervision()
    {
        await FiniteExitChild.WithDirectory("finite-exit-context-", async directory =>
        {
            using var child = FiniteExitChildTests.Start(directory, "deferred-context");
            var context = new DeferredContext();
            Task? supervision = null;
            try
            {
                using var readyDeadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                while (!File.Exists(Path.Combine(directory, "ready"))) await Task.Delay(10, readyDeadline.Token);
                var previous = SynchronizationContext.Current;
                try
                {
                    SynchronizationContext.SetSynchronizationContext(context);
                    supervision = FiniteExitChild.Wait(child, 0, "deferred context control", TimeSpan.FromMilliseconds(500));
                }
                finally { SynchronizationContext.SetSynchronizationContext(previous); }
                // Release actual work only after supervision starts, so a busy
                // runner cannot finish all phases before the controlled await.
                File.WriteAllText(Path.Combine(directory, "continue"), "continue");
                using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                await child.WaitForExitAsync(deadline.Token);
                int queuedBeforeDrain = context.Count;
                // Deliberately service the caller only after the finite child
                // finishes. Progress capture must run independently of it.
                while (!supervision.IsCompleted)
                {
                    context.Drain();
                    await Task.Delay(10, deadline.Token);
                }
                await supervision;
                Assert.Equal(0, queuedBeforeDrain);
                Assert.Equal(0, context.Count);
                File.Delete(Path.Combine(directory, "held"));
            }
            finally
            {
                if (!child.HasExited) { child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); }
                context.Drain();
                if (supervision is not null)
                    try { await supervision; } catch { /* Preserve the primary failure above. */ }
            }
        });
    }

    private sealed class DeferredContext : SynchronizationContext
    {
        private readonly ConcurrentQueue<(SendOrPostCallback Callback, object? State)> callbacks = new();
        internal int Count => callbacks.Count;
        public override void Post(SendOrPostCallback callback, object? state) => callbacks.Enqueue((callback, state));
        internal void Drain() { while (callbacks.TryDequeue(out var item)) item.Callback(item.State); }
    }
}
