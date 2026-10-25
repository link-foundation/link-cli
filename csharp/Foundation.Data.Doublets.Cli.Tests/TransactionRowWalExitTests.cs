using System.Diagnostics;
using System.Text.Json;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

// The OS stays alive; this finite process-loss model does not simulate power loss.
public class TransactionRowWalExitTests
{
    internal sealed record Row(uint Index, uint Source, uint Target);
    internal sealed record Image(Row[] Rows, KeyValuePair<uint, string>[] Names);

    private static Image Capture(TransactionsDecorator tx)
    {
        var any = tx.Constants.Any;
        return new(tx.All(new[] { any, any, any }).Select(raw => new Link<uint>(raw))
            .OrderBy(row => row.Index).Select(row => new Row(row.Index, row.Source, row.Target)).ToArray(),
            tx.CaptureNames().OrderBy(pair => pair.Key).ToArray());
    }

    private sealed class ExitStore(string path) : NamedTypesDecorator<uint>(path)
    {
        internal bool Armed;
        private uint Forward(IList<uint>? before, IList<uint>? after, WriteHandler<uint>? handler)
        {
            if (Armed)
            {
                // Persist real mapped data/names before the transaction wrapper
                // receives even the first observation of this operation.
                Flush();
                Environment.Exit(71);
            }
            return handler is null ? Constants.Continue : handler(before, after);
        }
        public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
            => base.Create(substitution, (before, after) => Forward(before, after, handler));
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
            => base.Update(restriction, substitution, (before, after) => Forward(before, after, handler));
        public override uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler)
            => base.Delete(restriction, (before, after) => Forward(before, after, handler));
    }

    internal static int Child(string directory, string operation, bool explicitOwner)
    {
        using var data = new ExitStore(Path.Combine(directory, "data.links"));
        using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
        using var tx = new TransactionsDecorator(data, log, rowUndoPolicy: RowUndoPolicy.WriteAheadImage);
        var first = tx.CreateAndUpdate(11u, 12u);
        tx.SetName(first, "original 雪");
        var dependent = tx.CreateAndUpdate(first, first);
        tx.SetName(dependent, "dependent");
        tx.SetName(0, "zero"); tx.SetName(77, "absent");
        File.WriteAllText(Path.Combine(directory, "expected.json"), JsonSerializer.Serialize(Capture(tx)));
        using var owner = explicitOwner ? tx.BeginTransaction() : null;
        data.Armed = true;
        switch (operation)
        {
            case "create": tx.Create(null, null); break;
            case "update": tx.Update(new[] { first }, new[] { first, 21u, 22u }, null); break;
            case "delete": tx.Delete(new[] { dependent }, null); break;
            case "cascade": tx.Delete(new[] { first }, null); break;
            default: throw new ArgumentException(operation);
        }
        throw new InvalidOperationException("The pre-observer exit seam was never reached.");
    }

    [Theory(Timeout = 45000)]
    [InlineData("create", false)]
    [InlineData("create", true)]
    [InlineData("update", false)]
    [InlineData("update", true)]
    [InlineData("delete", false)]
    [InlineData("delete", true)]
    [InlineData("cascade", false)]
    [InlineData("cascade", true)]
    public async Task RowEffectsBeforeTheFirstObservationAreUndoneAfterExit(string operation, bool explicitOwner)
    {
        var directory = Directory.CreateTempSubdirectory("row-wal-exit-").FullName;
        try
        {
            var info = new ProcessStartInfo("dotnet") { RedirectStandardOutput = true, RedirectStandardError = true };
            info.Environment["DOTNET_GCHeapHardLimit"] = "0x10000000";
            foreach (var argument in new[] { typeof(TransactionRowWalExitTests).Assembly.Location,
                "row-wal-exit", directory, operation, explicitOwner.ToString() }) info.ArgumentList.Add(argument);
            using var child = Process.Start(info)!;
            var stdout = child.StandardOutput.ReadToEndAsync(); var stderr = child.StandardError.ReadToEndAsync();
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            try { await child.WaitForExitAsync(deadline.Token); }
            catch (OperationCanceledException)
            {
                child.Kill(entireProcessTree: true); await child.WaitForExitAsync();
                await Task.WhenAll(stdout, stderr);
                throw new TimeoutException("Finite row-WAL child expired.");
            }
            Assert.True(child.ExitCode == 71, $"{operation}/{explicitOwner}: {await stdout}\n{await stderr}");
            var expected = JsonSerializer.Deserialize<Image>(File.ReadAllText(Path.Combine(directory, "expected.json")))!;
            for (var reopen = 0; reopen < 3; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var tx = new TransactionsDecorator(data, log);
                var actual = Capture(tx);
                Assert.Equal(expected.Rows, actual.Rows); Assert.Equal(expected.Names, actual.Names);
                if (reopen < 2)
                {
                    var following = tx.CreateAndUpdate(31u, 32u);
                    tx.SetName(following, "following " + reopen);
                    expected = Capture(tx);
                }
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
