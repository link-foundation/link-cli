using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class RecoveryValidationTests
{
    [Fact(Timeout = 20000)]
    public async Task RetainedAppliedMarkersReserveGapsWithoutTransitionPayloads()
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("recovery-retained-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "journal"));
            var row = data.CreateAndUpdate(21u, 22u);
            Write(log, Commit(1));
            Write(log, Commit(1));
            Write(log, "__transactions:applied:42");
            Write(log, "__transactions:applied:42");
            using var tx = new TransactionsDecorator(data, log);
            Assert.Equal(42, tx.AppliedSequence);
            Assert.Equal(42, tx.LastLoggedSequence);
            tx.Update(new[] { row }, new[] { row, 41u, 42u }, null);
            Assert.Equal(43, tx.LastLoggedSequence);
            tx.Recover();
            Assert.Equal(41u, tx.GetSource(row));
            Assert.Equal(42u, tx.GetTarget(row));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
    private static string Record(int owner, long sequence, uint after) =>
        $"__transactions:transition:v1|{owner:x32}|{sequence}|0|1|1,11,12|1,{after},22";
    private static string Commit(int owner) => $"__transactions:commit:{owner:x32}";
    private static string Abort(int owner) => $"__transactions:rollback:{owner:x32}";
    private static string[] Invalid(string kind)
    {
        var record = Record(1, 1, 21);
        return kind switch
        {
            "commit-abort" => [record, Commit(1), Abort(1)],
            "abort-commit" => [record, Abort(1), Commit(1)],
            "owners" => [record, Commit(1), Record(2, 1, 31), Commit(2)],
            "payloads" => [record, Record(1, 1, 31), Commit(1)],
            "timestamp" => [record, record.Replace("|1|0|1|", "|1|1|1|", StringComparison.Ordinal), Commit(1)],
            "zero-sequence" => [Record(1, 0, 21), Commit(1)],
            "negative-sequence" => [Record(1, -1, 21), Commit(1)],
            "zero-owner" => [Record(0, 1, 21), Commit(0)],
            "zero-commit" => [Commit(0)],
            "zero-abort" => [Abort(0)],
            "retained-contradiction" => [Commit(1), Abort(1)],
            "malformed-transition" => ["__transactions:transition:invalid"],
            "malformed-commit" => ["__transactions:commit:invalid"],
            "malformed-applied" => ["__transactions:applied:0"],
            _ => throw new ArgumentException(kind)
        };
    }
    private static uint Write(NamedTypesDecorator<uint> log, string record)
    {
        var row = log.CreateAndUpdate(0u, 0u);
        log.SetName(row, record);
        return row;
    }
    private static string[] Names(NamedTypesDecorator<uint> log)
    {
        var any = log.Constants.Any;
        return log.All(new Link<uint>(any, any, any))
            .Select(row => log.GetName(Assert.IsAssignableFrom<IList<uint>>(row)[0]))
            .Where(name => name is not null).Select(name => name!).ToArray();
    }
    [Theory(Timeout = 20000)]
    [InlineData("commit-abort")]
    [InlineData("abort-commit")]
    [InlineData("owners")]
    [InlineData("payloads")]
    [InlineData("timestamp")]
    [InlineData("zero-sequence")]
    [InlineData("negative-sequence")]
    [InlineData("zero-owner")]
    [InlineData("zero-commit")]
    [InlineData("zero-abort")]
    [InlineData("retained-contradiction")]
    [InlineData("malformed-transition")]
    [InlineData("malformed-commit")]
    [InlineData("malformed-applied")]
    public async Task RejectBeforeReplayAndRetainAdmissionUntilRecoveryRetry(string kind)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("recovery-validation-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "journal"));
            var row = data.CreateAndUpdate(11u, 12u);
            using var tx = new TransactionsDecorator(data, log, trace: true);
            // Malformed input must preserve populated live indexes as well as rows.
            if (kind.StartsWith("malformed-", StringComparison.Ordinal))
            {
                tx.Update(new[] { row }, new[] { row, 31u, 32u }, null);
                tx.Update(new[] { row }, new[] { row, 11u, 12u }, null);
                Assert.NotEmpty(tx.Log);
            }
            var previous = tx.Log.ToArray();
            var previousSequence = tx.LastLoggedSequence;
            if (previous.Length > 0)
            {
                // Externally replace the recognized journal, keeping live
                // history populated until validation accepts the new input.
                var any = log.Constants.Any;
                foreach (var raw in log.All(new Link<uint>(any, any, any)).ToArray())
                {
                    var marker = Assert.IsAssignableFrom<IList<uint>>(raw)[0];
                    log.SetName(marker, "unrelated saved history " + marker);
                }
            }
            var records = Invalid(kind).Select(record => Write(log, record)).ToArray();
            var names = Names(log);
            for (var attempt = 0; attempt < 2; attempt++)
            {
                Assert.Throws<InvalidDataException>(tx.Recover);
                Assert.Equal(11u, tx.GetSource(row));
                Assert.Equal(12u, tx.GetTarget(row));
                Assert.Equal(names, Names(log));
                Assert.Equal(previous, tx.Log);
                Assert.Equal(previousSequence, tx.LastLoggedSequence);
                Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                Assert.Throws<InvalidOperationException>(tx.BeginVersionReplay);
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(41u, 42u));
                Assert.Throws<InvalidOperationException>(() => tx.Update(new[] { row }, new[] { row, 41u, 42u }, null));
                Assert.Throws<InvalidOperationException>(() => tx.Delete(new[] { row }, null));
                const string payload = "v1|00000000000000000000000000000001|1|0|1|1,11,12|1,21,22";
                Assert.True(Transition<uint>.TryParse(payload, out var change));
                Assert.Throws<InvalidOperationException>(() => tx.ApplyTransition(change));
                Assert.Throws<InvalidOperationException>(() => tx.RevertTransition(change));
                Assert.Throws<InvalidDataException>(tx.Rollback);
                Assert.Equal(11u, tx.GetSource(row));
                Assert.Equal(names, Names(log));
            }
            foreach (var marker in records) log.SetName(marker, "repaired unrelated journal row " + marker);
            tx.Recover();
            tx.Update(new[] { row }, new[] { row, 41u, 42u }, null);
            for (var attempt = 0; attempt < 2; attempt++)
            {
                tx.Recover();
                Assert.Equal(41u, tx.GetSource(row));
                Assert.Equal(42u, tx.GetTarget(row));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory(Timeout = 20000)]
    [InlineData("commit")]
    [InlineData("abort")]
    [InlineData("pending")]
    public async Task IdenticalRecordsAndDecisionsAreIdempotent(string decision)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("recovery-idempotent-").FullName;
        var dataPath = Path.Combine(directory, "data");
        var logPath = Path.Combine(directory, "journal");
        try
        {
            uint row;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            {
                row = data.CreateAndUpdate(decision == "abort" ? 11u : 21u, decision == "abort" ? 12u : 22u);
                Write(log, Record(1, 3, 21));
                Write(log, Record(1, 3, 21));
                Write(log, Record(1, 3, 21).Replace("|3|0|", "|03|00|", StringComparison.Ordinal) + "|legacy-extension");
                Write(log, "unrelated application name");
                if (decision != "pending")
                {
                    Write(log, decision == "commit" ? Commit(1) : Abort(1));
                    Write(log, decision == "commit" ? Commit(1) : Abort(1));
                }
                using var tx = new TransactionsDecorator(data, log);
                Assert.Single(tx.Log);
                for (var attempt = 0; attempt < 2; attempt++)
                {
                    tx.Recover();
                    Assert.Equal(decision == "commit" ? 21u : 11u, tx.GetSource(row));
                    Assert.Equal(decision == "commit" ? 22u : 12u, tx.GetTarget(row));
                }
                tx.Update(new[] { row }, new[] { row, 41u, 42u }, null);
                Assert.Equal(4, tx.LastLoggedSequence);
            }
            for (var attempt = 0; attempt < 2; attempt++)
            {
                using var data = new NamedTypesDecorator<uint>(dataPath);
                using var log = new NamedTypesDecorator<uint>(logPath);
                using var tx = new TransactionsDecorator(data, log);
                Assert.Equal(41u, tx.GetSource(row));
                Assert.Equal(42u, tx.GetTarget(row));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
