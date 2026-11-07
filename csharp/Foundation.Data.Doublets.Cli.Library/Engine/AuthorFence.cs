using System.Globalization;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record AuthorFenceOptions(int MaxEntries = 4096, int MaxBytes = 1_048_576, bool RequireDurableFlush = false);
public sealed record AuthorClaim(string Author, string Token, ulong Epoch);
public sealed record AuthorFenceSnapshot(ulong Generation, ulong Epoch, IReadOnlyList<AuthorClaim> Claims);

/// <summary>Optional explicit takeover with permanent claim receipts. Every
/// guarded data CAS compares the current fence atomically. Caller-owned store;
/// no expiry, election, permission checks or protection from bypassing writers.</summary>
public sealed class AuthorFence
{
    private readonly IAtomicJournalStore _store;
    private readonly string _key, _author, _header;
    private readonly AuthorFenceOptions _options;
    public AuthorFence(IAtomicJournalStore store, string fenceKey, string author, AuthorFenceOptions? options = null)
    {
        options ??= new(); Identity(author);
        if (store is null || !Key(fenceKey) || options.MaxEntries is < 2 or > 4096 || options.MaxBytes is < 1 or > 1_048_576 ||
            (options.RequireDurableFlush && !store.SupportsDurableFlush)) throw Invalid();
        _store = store; _key = fenceKey; _author = author; _options = options;
        _header = JsonSerializer.Serialize(new[] { "author-fence1", author });
    }
    private static InvalidDataException Invalid() => new("Invalid author fence state or request.");
    private static InvalidOperationException Fenced() => new("Author owner is fenced by a later takeover.");
    private static bool Key(string value) => value is not null && Regex.IsMatch(value, "\\A[A-Za-z0-9_.-]{1,128}\\z", RegexOptions.CultureInvariant);
    private static string Text(ulong value) => value.ToString(CultureInfo.InvariantCulture);
    private static void Identity(string value)
    {
        if (value is null || value.Length != 32 || value.All(c => c == '0') || value.Any(c => !(c is >= '0' and <= '9' or >= 'a' and <= 'f'))) throw Invalid();
    }
    private void Admit(IReadOnlyList<string> entries)
    {
        if (entries is null || entries.Count > _options.MaxEntries) throw Invalid();
        int bytes = 0;
        // Canonical fields are ASCII: count real UTF-8 bytes before JSON parsing.
        foreach (string entry in entries) if (entry is null || entry.Length > 256 || entry.Any(c => c > 127) || (bytes += entry.Length) > _options.MaxBytes) throw Invalid();
    }
    private (JournalVersion Expected, List<AuthorClaim> Claims) Parse(JournalVersion value)
    {
        if (value is null) throw Invalid(); Admit(value.Entries);
        var expected = value with { Entries = value.Entries.ToArray() }; var claims = new List<AuthorClaim>(); var tokens = new HashSet<string>(StringComparer.Ordinal);
        if (expected.Generation == 0) { if (expected.Entries.Count != 0) throw Invalid(); }
        else
        {
            if (expected.Entries.Count == 0 || expected.Entries[0] != _header) throw Invalid();
            foreach (string text in expected.Entries.Skip(1))
            {
                using var document = JsonDocument.Parse(text); var root = document.RootElement;
                if (root.ValueKind != JsonValueKind.Array || root.GetArrayLength() != 3 || root.EnumerateArray().Any(v => v.ValueKind != JsonValueKind.String)) throw Invalid();
                var entry = root.EnumerateArray().Select(v => v.GetString()!).ToArray();
                if (entry[0] != "claim-author1" || JsonSerializer.Serialize(entry) != text) throw Invalid(); Identity(entry[1]);
                ulong epoch = (ulong)claims.Count + 1;
                if (entry[2] != Text(epoch) || !tokens.Add(entry[1])) throw Invalid();
                claims.Add(new(_author, entry[1], epoch));
            }
        }
        return (expected, claims);
    }
    private (JournalVersion Expected, List<AuthorClaim> Claims, IReadOnlyList<JournalVersion> Values) Read(IReadOnlyList<string> keys)
    {
        var values = _store.Read(new[] { _key }.Concat(keys).ToArray());
        if (values is null || values.Count != keys.Count + 1) throw Invalid(); var state = Parse(values[0]);
        return (state.Expected, state.Claims, values.Skip(1).Select(v => v with { Entries = v.Entries.ToArray() }).ToArray());
    }
    public AuthorFenceSnapshot Snapshot()
    {
        var state = Read([]); return new(state.Expected.Generation, (ulong)state.Claims.Count, state.Claims.ToArray());
    }
    /// <summary>One CAS attempt. Retry the original predecessor/token after
    /// unknown I/O. Retrying a superseded claim never retakes ownership.</summary>
    public AuthorClaim Claim(ulong expectedEpoch, string token)
    {
        Identity(token); var state = Read([]); var retained = state.Claims.Find(v => v.Token == token);
        if (retained is not null)
        {
            if (retained.Epoch != (ulong)state.Claims.Count || expectedEpoch == ulong.MaxValue || retained.Epoch != expectedEpoch + 1) throw Fenced();
            return retained;
        }
        if (expectedEpoch != (ulong)state.Claims.Count) throw new InvalidOperationException("Conflict: author epoch changed.");
        if (state.Expected.Generation == ulong.MaxValue) throw Invalid();
        var result = new AuthorClaim(_author, token, expectedEpoch + 1); var entries = state.Expected.Entries.ToList(); if (entries.Count == 0) entries.Add(_header);
        entries.Add(JsonSerializer.Serialize(new[] { "claim-author1", token, Text(result.Epoch) })); Admit(entries);
        bool committed;
        try { committed = _store.CompareExchange([_key], [state.Expected], [entries]); }
        catch (Exception cause) { throw new IOException("Indeterminate author takeover; retry the original predecessor/token.", cause); }
        if (!committed) throw new InvalidOperationException("Conflict: author epoch changed."); return result;
    }
    public GuardedJournalStore Bind(AuthorClaim claim)
    {
        if (claim is null || claim.Author != _author || claim.Epoch == 0) throw Invalid(); Identity(claim.Token);
        return new(this, claim);
    }
    public sealed class GuardedJournalStore : IAtomicJournalStore
    {
        private readonly AuthorFence _owner; private readonly AuthorClaim _claim;
        internal GuardedJournalStore(AuthorFence owner, AuthorClaim claim) { _owner = owner; _claim = claim; }
        public IAtomicJournalStore Store => _owner._store;
        public bool SupportsDurableFlush => Store.SupportsDurableFlush;
        private string[] Keys(IReadOnlyList<string> keys)
        {
            if (keys is null || keys.Count is < 1 or > 31 || keys.Distinct(StringComparer.Ordinal).Count() != keys.Count || keys.Any(k => !Key(k) || k == _owner._key)) throw Invalid();
            return keys.ToArray();
        }
        private (JournalVersion Expected, IReadOnlyList<JournalVersion> Values) Current(IReadOnlyList<string> keys)
        {
            var state = _owner.Read(keys);
            if (state.Claims.Count == 0 || state.Claims[^1] != _claim) throw Fenced(); return (state.Expected, state.Values);
        }
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => Current(Keys(keys)).Values;
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            var selected = Keys(keys);
            if (expected is null || updates is null || expected.Count != selected.Length || updates.Count != selected.Length) throw Invalid();
            var versions = expected.Select(v => v with { Entries = v.Entries.ToArray() }).ToArray();
            var writes = updates.Select(v => (IReadOnlyList<string>?)v?.ToArray()).ToArray(); var state = Current([]);
            return Store.CompareExchange(new[] { _owner._key }.Concat(selected).ToArray(), new[] { state.Expected }.Concat(versions).ToArray(), new IReadOnlyList<string>?[] { null }.Concat(writes).ToArray());
        }
    }
}
