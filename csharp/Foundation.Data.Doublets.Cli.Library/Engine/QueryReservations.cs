using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

// FIFO execution order with constant-time explicit-address retirement.
internal sealed class QueryReservations<T> where T : IUnsignedNumber<T>
{
    private readonly LinkedList<T> _order = new();
    private readonly Dictionary<T, LinkedListNode<T>> _nodes = new();
    internal IEnumerable<T> Addresses => _order;
    internal bool TryPeek(out T value)
    {
        if (_order.First is { } node) { value = node.Value; return true; }
        value = T.Zero;
        return false;
    }
    internal void Add(T value)
    {
        if (!_nodes.ContainsKey(value)) _nodes.Add(value, _order.AddLast(value));
    }
    internal void Remove(T value)
    {
        if (_nodes.Remove(value, out var node)) _order.Remove(node);
    }
}
