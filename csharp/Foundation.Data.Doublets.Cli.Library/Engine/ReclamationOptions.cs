namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Complete per-namespace scan limits and independently selected cache.
/// Zero cache bytes retains no objects; scan bytes use JournalScanOptions accounting.</summary>
public sealed record ReclamationOptions(int MaxKeys = 4096, long MaxBytes = 8_388_608, int PageKeys = 128, long CacheBytes = 0);
public sealed record ReclamationResult(ulong Epoch, long Examined, long Deleted);
