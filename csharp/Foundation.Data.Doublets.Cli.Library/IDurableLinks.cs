namespace Foundation.Data.Doublets.Cli;

/// <summary>An optional persistence boundary for a links decorator.</summary>
public interface IDurableLinks
{
    /// <summary>Whether this instance can flush all of its backing storage.</summary>
    bool SupportsDurableFlush { get; }
    /// <summary>Flushes all backing files to stable storage, or throws.</summary>
    void Flush();
}
