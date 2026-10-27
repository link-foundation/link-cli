namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class ReclaimingPartitionedJournalStore
{
    private IReadOnlyList<string>? ReadObject(string partition, ReclamationCodec.Reference reference)
    {
        if (_cache?.Get(partition, reference.Key, reference.Generation) is { } cached)
        { JournalChunks.ValidateChunk(cached, reference.Records); return cached; }
        var data = ReclamationCodec.Read(_partitions[partition], new[] { reference.Key })[0];
        if (data.Generation == 0) return null;
        if (data.Generation != reference.Generation || ReclamationCodec.Address(reference.Epoch, data.Entries) != reference.Key) throw new InvalidDataException("Modified immutable reclamation object.");
        JournalChunks.ValidateChunk(data.Entries, reference.Records);
        _cache?.Remember(partition, reference.Key, reference.Generation, data.Entries);
        return data.Entries;
    }
    private (string Key, ulong Generation) StageObject(string partition, ulong epoch, IReadOnlyList<string> records)
    {
        // Never reuse an older epoch's prefix, including a verified cached copy.
        var key = ReclamationCodec.Address(epoch, records); var provider = _partitions[partition];
        var item = ReclamationCodec.Read(provider, new[] { key })[0];
        if (item.Generation == 0)
        {
            provider.CompareExchange(new[] { key }, new[] { item }, new IReadOnlyList<string>?[] { records });
            item = ReclamationCodec.Read(provider, new[] { key })[0];
        }
        if (item.Generation == 0 || !item.Entries.SequenceEqual(records)) throw new InvalidDataException("Immutable object collision or staging conflict.");
        return (key, item.Generation);
    }
}
