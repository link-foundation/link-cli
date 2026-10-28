namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Decoded key bytes plus eight generation bytes and UTF-8 record bytes
/// plus one separator per record. Container overhead also depends on record count.</summary>
public sealed record JournalScanOptions(int MaxKeys = 4096, long MaxBytes = 8_388_608, int PageKeys = 128, bool KeysOnly = false);
public sealed record JournalNamespaceEntry(string Key, JournalVersion Version);

/// <summary>Optional complete bounded namespace snapshot, or an error; never
/// partial success. Generations are nonzero and never reused, including after
/// deletion. A delete error has an unknown outcome and must not be blindly retried.
/// This capability does not supply object liveness or reclamation policy.</summary>
public interface IJournalMaintenanceStore : IAtomicJournalStore
{
    IReadOnlyList<JournalNamespaceEntry> ScanNamespace(JournalScanOptions? options = null);
    bool CompareDelete(string key, ulong expectedGeneration);
}
