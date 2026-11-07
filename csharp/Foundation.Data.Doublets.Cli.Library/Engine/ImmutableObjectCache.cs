using System.Text;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Weighted LRU for checked immutable objects; never caches catalog state.</summary>
internal sealed class ImmutableObjectCache(long capacity)
{
    private sealed record Entry((string Partition, string Key, ulong Generation) Key, long Size, string[] Records);
    private readonly Dictionary<(string, string, ulong), LinkedListNode<Entry>> _objects = new();
    private readonly LinkedList<Entry> _order = new();
    private readonly object _gate = new();
    private long _used;
    internal string[]? Get(string partition, string key, ulong generation)
    {
        lock (_gate)
        {
            if (!_objects.TryGetValue((partition, key, generation), out var node)) return null;
            _order.Remove(node); _order.AddLast(node); return node.Value.Records.ToArray();
        }
    }
    internal void Remember(string partition, string key, ulong generation, IReadOnlyList<string> entries)
    {
        long size = 1;
        foreach (var entry in entries)
        {
            long bytes = (long)Encoding.UTF8.GetByteCount(entry) + 1;
            if (bytes > capacity - size) return;
            size += bytes;
        }
        lock (_gate)
        {
            var identity = (partition, key, generation);
            if (_objects.Remove(identity, out var old)) { _used -= old.Value.Size; _order.Remove(old); }
            while (size > capacity - _used)
            {
                var first = _order.First!; _used -= first.Value.Size;
                _objects.Remove(first.Value.Key); _order.RemoveFirst();
            }
            var node = _order.AddLast(new Entry(identity, size, entries.ToArray()));
            _objects.Add(identity, node); _used += size;
        }
    }
}
