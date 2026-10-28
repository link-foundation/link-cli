using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    public RowUndoPolicy RowUndoPolicy { get; }

    private Link<TLinkAddress>[] CaptureRows() => _inner.All(new Link<TLinkAddress>(
        _inner.Constants.Any, _inner.Constants.Any, _inner.Constants.Any))
        .Select(row => new Link<TLinkAddress>(row)).OrderBy(row => row.Index).ToArray();

    private void PrepareRowUndo(Transaction transaction)
    {
        if (RowUndoPolicy != RowUndoPolicy.WriteAheadImage || transaction.Image is not null) return;
        var image = new TransactionUndoImage<TLinkAddress>(transaction.Id, _sequenceCounter,
            CaptureRows(), new Dictionary<TLinkAddress, string>(_inner.CaptureNames()));
        var payload = image.Serialize();
        _ = TransactionUndoImage<TLinkAddress>.Parse(payload);
        transaction.Image = image;
        WriteMarker(TransitionNamePrefix + payload);
        _flushLog();
        Trace($"Prepared row undo image tx={transaction.Id:N}.");
    }

    // A complete abort source alone cannot make committed history complete.
    // Compare the image plus recorded changes with the actual logical result,
    // and append corrections before returning or preparing a version intent.
    private void CompleteImageObservations(Transaction transaction)
    {
        if (transaction.Image is not { } image) return;
        var predicted = image.Rows.ToDictionary(row => row.Index);
        var names = image.Names is null ? new Dictionary<TLinkAddress, string>() : new(image.Names);
        foreach (var record in transaction.Records)
        {
            if (!record.NameOnly)
            {
                if (record.Kind == TransitionKind.Delete) predicted.Remove(record.Before.Index);
                else predicted[record.After.Index] = record.After;
            }
            foreach (var change in record.Names) names.Remove(change.Index);
            foreach (var change in record.Names) if (change.After is { } name) names[change.Index] = name;
        }
        var actual = CaptureRows().ToDictionary(row => row.Index);
        var changes = predicted.Keys.Concat(actual.Keys).Distinct().Order().Where(index =>
            !predicted.TryGetValue(index, out var before) || !actual.TryGetValue(index, out var after) || !before.Equals(after))
            .Select(index => (Before: predicted.TryGetValue(index, out var before) ? (Link<TLinkAddress>?)before : null,
                After: actual.TryGetValue(index, out var after) ? (Link<TLinkAddress>?)after : null));
        RecordTransitions(transaction, changes, names, _inner.CaptureNames());
    }

    private void RestoreImage(TransactionUndoImage<TLinkAddress> image)
    {
        var zero = TLinkAddress.Zero;
        // Empty placeholders break usages/uniqueness before removing allocations.
        foreach (var row in CaptureRows())
            if (row.Source != zero || row.Target != zero)
                _inner.Update(new[] { row.Index }, new[] { row.Index, zero, zero }, null);
        var wanted = image.Rows.Select(row => row.Index).ToHashSet();
        foreach (var row in CaptureRows()) if (!wanted.Contains(row.Index)) DeleteIfExists(row.Index);
        foreach (var row in image.Rows) _inner.EnsureCreated(row.Index);
        // Dense address allocators can create intermediate placeholders.
        foreach (var row in CaptureRows()) if (!wanted.Contains(row.Index)) DeleteIfExists(row.Index);
        foreach (var row in image.Rows) RestoreLink(row);
        if (image.Names is not null)
        {
            foreach (var index in _inner.CaptureNames().Keys) _inner.RemoveName(index);
            foreach (var pair in image.Names) _inner.SetName(pair.Key, pair.Value);
        }
        if (!CaptureRows().SequenceEqual(image.Rows) || (image.Names is not null
            && !_inner.CaptureNames().OrderBy(pair => pair.Key).SequenceEqual(image.Names.OrderBy(pair => pair.Key))))
            throw new InvalidDataException("Storage did not restore the complete undo image.");
    }
}
