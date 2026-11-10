using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli;

// A retained preview interface must never reach a freed heap mapping.
internal sealed class QueryPreviewLease<T>(INamedTypesLinks<T> inner) : INamedTypesLinks<T>
    where T : IUnsignedNumber<T>
{
    private INamedTypesLinks<T>? _inner = inner;
    private INamedTypesLinks<T> Open => _inner ?? throw new ObjectDisposedException(nameof(QueryPreviewLease<T>));
    internal void Close() => _inner = null;
    public LinksConstants<T> Constants => Open.Constants;
    public T Count(IList<T>? restriction) => Open.Count(restriction);
    public T Each(IList<T>? restriction, ReadHandler<T>? handler) => Open.Each(restriction, handler);
    public T Create(IList<T>? substitution, WriteHandler<T>? handler) => Open.Create(substitution, handler);
    public T Update(IList<T>? restriction, IList<T>? substitution, WriteHandler<T>? handler) => Open.Update(restriction, substitution, handler);
    public T Delete(IList<T>? restriction, WriteHandler<T>? handler) => Open.Delete(restriction, handler);
    public string? GetName(T link) => Open.GetName(link);
    public T SetName(T link, string name) => Open.SetName(link, name);
    public T GetByName(string name) => Open.GetByName(name);
    public void RemoveName(T link) => Open.RemoveName(link);
    public IReadOnlyDictionary<T, string> CaptureNames() => Open.CaptureNames();
}
