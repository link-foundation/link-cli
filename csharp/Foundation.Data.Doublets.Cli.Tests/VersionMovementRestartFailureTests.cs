using System.Diagnostics;
using Platform.Data;
using Platform.Data.Doublets;
using Xunit.Abstractions;
using static Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMovementRestartFailureTests(
  VersionMovementRestartFailureTests.InterruptedMovement fixture, ITestOutputHelper output)
  : IClassFixture<VersionMovementRestartFailureTests.InterruptedMovement>
{
    public sealed class InterruptedMovement : IAsyncLifetime
    {
        private readonly string _directory = Directory.CreateTempSubdirectory("movement-restart-image-").FullName;
        internal (uint Id, long Original, long Attempted) Position { get; private set; }

        // All cases start at the same persisted cut. Each recovery runs on
        // its own copy, so no case can change the fixture or another case.
        public async Task InitializeAsync() =>
            Position = await VersionProcessExitTests.ExitChild(_directory, "intent-flush-after", true, "movement-exit");

        internal void CopyTo(string directory)
        {
            foreach (var file in Directory.EnumerateFiles(_directory))
                File.Copy(file, Path.Combine(directory, Path.GetFileName(file)));
        }

        public Task DisposeAsync()
        {
            Directory.Delete(_directory, recursive: true);
            return Task.CompletedTask;
        }
    }

    public static TheoryData<string> Cases
    {
        get
        {
            var cases = new TheoryData<string>();
            foreach (var phase in new[] { "current", "applied", "publication-flush", "intent-remove", "intent-delete", "cleanup-flush" })
                foreach (var edge in new[] { "before", "after" }) cases.Add(phase + "-" + edge);
            return cases;
        }
    }

    [Theory(Timeout = 30000)]
    [MemberData(nameof(Cases))]
    public async Task FailedConstructorKeepsRecoveryOwnerReachable(string point)
    {
        await FiniteExitChild.WithDirectory("movement-restart-failure-", directory =>
        {
            var elapsed = Stopwatch.StartNew();
            var verbose = Environment.GetEnvironmentVariable("LINK_CSHARP_RECOVERY_TRACE") == "1";
            void Phase(string phase)
            {
                if (verbose)
                    output.WriteLine($"{point}: {phase} at {elapsed.ElapsedMilliseconds} ms (queued work: {ThreadPool.PendingWorkItemCount})");
            }

            fixture.CopyTo(directory);
            var (id, original, attempted) = fixture.Position;
            Phase("copied interrupted movement");
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            var fault = new Fault { Point = point, Flushes = 1 };
            if (verbose) fault.OnCheck = Phase;
            using var branches = new Metadata(Path.Combine(directory, "branches.links"), fault);
            using var tx = new TransactionsDecorator(data, log);
            Phase("opened stores");
            try
            {
                var error = Assert.Throws<IOException>(() => new VersionControlDecorator(tx, branches));
                Phase("first recovery attempt returned");
                Assert.Contains(point, error.Message);
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(71u, 72u));
                Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                Assert.Throws<InvalidOperationException>(() => tx.Shutdown());
                fault.Point = "flush-before";
                Assert.Throws<IOException>(() => new VersionControlDecorator(tx, branches));
                Phase("second recovery attempt returned");
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(71u, 72u));
                using var recovered = new VersionControlDecorator(tx, branches);
                Phase("recovered version");
                Assert.Equal(VersionMovementExitTests.Topic, recovered.CurrentBranch);
                Assert.Equal(original, recovered.CurrentSequence);
                Assert.Equal(11u, recovered.GetSource(id));
                Assert.Equal(attempted, tx.LastLoggedSequence);
                recovered.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                Assert.Equal(attempted + 1, recovered.CurrentSequence);
                data.Flush(); log.Flush(); branches.Flush();
                Phase("flushed recovered stores");
            }
            finally
            {
                // Release fixture resources even when the pre-fix orphaned
                // lease makes the assertions fail. No following writes occur.
                tx.FinishVersionReplay();
                Phase("released recovery owner");
            }
            return Task.CompletedTask;
        });
    }
}
