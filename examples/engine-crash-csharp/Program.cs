// The C# twin of rust/examples/engine_crash_writer.rs: a writer to be killed by
// .github/scripts/docker-crash.mjs, speaking the same protocol.
//
//   write  PRIMARY REPLICA [COMMITS]  print `ack R` once revision R is durable on
//                                     both files; `error ...` and exit 1 on a
//                                     failure such as a full disk
//   verify JOURNAL                    open it as a restarting writer does and
//                                     print `revision R`, or `refused ...` and exit 2
//   repair JOURNAL [COPY...]          rebuild it from its valid prefix or a copy:
//                                     `repaired R changed true|false`
//
// Revision R holds links 1..=R, each pointing at itself; a torn transaction
// would leave a link pointing at nothing, which `verify` refuses.
using Foundation.Data.Doublets.Cli.Engine;

var options = new EngineOptions { RevisionEncoding = RevisionEncoding.Changes, RequireDurableFlush = true };
try
{
    switch (args)
    {
        case ["write", var primaryPath, var replicaPath, .. var rest] when rest.Length <= 1:
        {
            ulong? commits = rest.Length == 1 ? ulong.Parse(rest[0]) : null;
            using var primary = new FileCommitLog(primaryPath);
            using var replica = new FileCommitLog(replicaPath);
            var engine = new LinksEngine<ulong>(new ReplicatedCommitLog(primary, [replica], acknowledgements: 2), options);
            for (ulong written = 0; commits is null || written < commits; written++)
            {
                using var tx = engine.Begin();
                var id = tx.CreateLink(0, 0);
                tx.UpdateLink(id, id, id);
                Console.Out.WriteLine($"ack {tx.Commit()}");
                Console.Out.Flush();
            }
            return 0;
        }
        case ["verify", var path]:
        {
            string? reason;
            ulong revision = 0;
            try { (revision, reason) = Verify(path, options); }
            catch (Exception error) { reason = error.Message; }
            if (reason is not null)
            {
                Console.WriteLine($"refused {reason}");
                return 2;
            }
            Console.WriteLine($"revision {revision}");
            return 0;
        }
        case ["repair", var path, .. var copies]:
        {
            var repair = EngineJournal<ulong>.RepairFile(path, copies, options);
            Console.WriteLine($"repaired {repair.Recovery.Revision} changed {(repair.Changed ? "true" : "false")}");
            return 0;
        }
        default:
            Console.WriteLine("error usage: write PRIMARY REPLICA [COMMITS] | verify JOURNAL | repair JOURNAL [COPY...]");
            return 1;
    }
}
catch (Exception error)
{
    Console.WriteLine($"error {error.Message.ReplaceLineEndings(" ")}");
    return 1;
}

static (ulong Revision, string? Reason) Verify(string path, EngineOptions options)
{
    using var log = new FileCommitLog(path);
    var snapshot = new LinksEngine<ulong>(log, options).Snapshot();
    if ((ulong)snapshot.Rows.Count != snapshot.Revision)
        return (snapshot.Revision, $"{snapshot.Rows.Count} links at revision {snapshot.Revision}");
    ulong expected = 1;
    foreach (var (index, row) in snapshot.Rows)
    {
        if (index != expected || row.Link.Source != expected || row.Link.Target != expected)
            return (snapshot.Revision, $"torn transaction: link {index}:{row.Link.Source}:{row.Link.Target} where {expected}:{expected}:{expected} was committed");
        expected++;
    }
    return (snapshot.Revision, null);
}
