using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Engine;

public enum CausalConflictPolicy { Reject, MaxDot }
public sealed record CausalProjectionOptions(int MaxVersions = 4096, int MaxBytes = 8_388_608);

/// <summary>Selected owned snapshot view. Retains complete causal siblings,
/// including deletion markers, independently of the explicit visible policy.</summary>
public sealed class CausalProjection
{
    private readonly CausalVersion[] _versions, _selected;
    private static InvalidDataException Invalid() => new("Invalid causal projection.");
    private static CausalVersion Own(CausalVersion value) => CausalVersion.Decode(value.Encode());

    public CausalProjection(IReadOnlyList<CausalVersion> values, CausalConflictPolicy policy,
        CausalProjectionOptions? options = null)
    {
        options ??= new();
        if (policy is not (CausalConflictPolicy.Reject or CausalConflictPolicy.MaxDot) ||
            options.MaxVersions is < 1 or > 4096 || options.MaxBytes is < 1 or > 8_388_608 ||
            values is null || values.Count > options.MaxVersions) throw Invalid();
        var groups = new SortedDictionary<ulong, List<CausalVersion>>();
        var dots = new Dictionary<CausalDot, string>(); int bytes = 0;
        foreach (var input in values)
        {
            if (input is null) throw Invalid();
            string wire = input.Encode(); bytes += wire.Length;
            if (bytes > options.MaxBytes) throw Invalid();
            var value = CausalVersion.Decode(wire);
            if (dots.TryGetValue(value.Dot, out var previous) && previous != wire) throw Invalid();
            dots[value.Dot] = wire;
            if (!groups.TryGetValue(value.Address, out var group)) groups[value.Address] = group = [];
            if (group.Count == 64) throw Invalid();
            group.Add(value);
        }
        var retained = new List<CausalVersion>(); var selected = new List<CausalVersion>();
        foreach (var group in groups.Values)
        {
            var siblings = CausalVersion.Merge(group, []);
            if (policy == CausalConflictPolicy.Reject && siblings.Length != 1)
                throw new InvalidOperationException("Conflict: concurrent causal siblings.");
            retained.AddRange(siblings); selected.Add(siblings[^1]);
        }
        _versions = retained.ToArray(); _selected = selected.ToArray();
    }

    public CausalVersion[] Versions() => _versions.Select(Own).ToArray();
    public CausalVersion[] Selected() => _selected.Select(Own).ToArray();
    public EngineRow<ulong>[] Rows() => _selected.Where(value => value.Payload is not null)
        .Select(value => new EngineRow<ulong>(new Link<ulong>(value.Address,
            value.Payload!.Source, value.Payload.Target), value.Payload.Name)).ToArray();
}
