using System.Reflection;
using System.Security.Cryptography;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>An error as Hasura reports it.</summary>
public sealed class GqlError(string message, string code, string path) : Exception(message)
{
    public string Code { get; } = code;
    public string ErrorPath { get; } = path;

    public static GqlError Validation(string message, string path) => new(message, "validation-failed", path);

    public static GqlError Write(string message)
    {
        var code = message.Contains("orbidden", StringComparison.Ordinal)
            ? "permission-error"
            : message.Contains("violation", StringComparison.Ordinal) ? "constraint-violation" : "unexpected";
        return new GqlError(message, code, "$");
    }

    public Json ToJson() => Json.Object(("errors", Json.Array([Json.Object(
        ("message", Json.Str(Message)),
        ("extensions", Json.Object(("path", Json.Str(ErrorPath)), ("code", Json.Str(Code)))))])));
}

/// <summary>What the transport learned of who is asking; a null user is a guest.</summary>
public readonly record struct Auth(long? User, bool Admin);

/// <summary>What a server keeps for all its requests.</summary>
public sealed class Settings
{
    public Settings(byte[] secret, string? adminSecret)
    {
        Secret = secret;
        AdminSecret = adminSecret;
        using var resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("introspection.json")
            ?? throw new InvalidOperationException("The recorded introspection is not embedded.");
        using var reader = new StreamReader(resource);
        Introspection = Json.Parse(reader.ReadToEnd()).Get("data") ?? Json.Null;
    }

    public byte[] Secret { get; }
    public string? AdminSecret { get; }
    public Json Introspection { get; }

    /// <summary>A secret for tokens that need not outlive this process.</summary>
    public static byte[] RandomSecret() => RandomNumberGenerator.GetBytes(32);

    /// <summary>The subject of <c>Authorization: Bearer &lt;jwt&gt;</c> and of the admin secret.</summary>
    /// <exception cref="FormatException">The token or the admin secret is not valid.</exception>
    public Auth Authorize(string? authorization, string? adminSecret)
    {
        var admin = false;
        if (adminSecret is not null)
        {
            if (AdminSecret != adminSecret)
            {
                throw new FormatException("invalid x-hasura-admin-secret");
            }
            admin = true;
        }
        var token = authorization is null
            ? ""
            : (authorization.StartsWith("Bearer ", StringComparison.Ordinal) ? authorization[7..] : authorization).Trim();
        if (token.Length == 0)
        {
            return new Auth(null, admin);
        }
        try
        {
            return new Auth(unchecked((long)Crypto.VerifyToken(Secret, token, Exec.Now())), admin);
        }
        catch (FormatException error)
        {
            throw new FormatException($"Could not verify JWT: {error.Message}", error);
        }
    }
}

/// <summary>One request's run over the store: authorisation, the root fields and introspection.</summary>
public sealed partial class Exec
{
    private readonly DeepStore _store;
    private readonly Settings _settings;
    private readonly Document _document;
    private readonly Json _variables;
    private readonly Auth _auth;

    /// <summary>The subject's address in the store; 0 is a guest.</summary>
    private readonly uint _subject;

    /// <summary>The remote columns of a relation's filter, kept for one root field.</summary>
    private readonly Dictionary<string, HashSet<string>> _memo = [];

    private Exec(DeepStore store, Settings settings, Document document, Json variables, Auth auth, uint subject)
    {
        _store = store;
        _settings = settings;
        _document = document;
        _variables = variables;
        _auth = auth;
        _subject = subject;
    }

    internal static ulong Now() => (ulong)DateTimeOffset.UtcNow.ToUnixTimeSeconds();

    /// <summary>Runs an operation and returns the whole response, and whether it wrote.</summary>
    public static (Json Response, bool Wrote) Execute(DeepStore store, Settings settings, string query, string? operation, Json variables, Auth auth)
    {
        Document document;
        Operation selected;
        try
        {
            document = Document.Parse(query);
            selected = document.OperationNamed(operation);
        }
        catch (FormatException error)
        {
            return (GqlError.Validation(error.Message, "$.query").ToJson(), false);
        }
        // A variable a request leaves out takes the operation's default.
        var defaults = variables.Kind == JsonKind.Object ? variables.Pairs.ToList() : [];
        foreach (var (name, fallback) in selected.Variables)
        {
            if (fallback is not null && variables.Get(name) is null)
            {
                defaults.Add(new KeyValuePair<string, Json>(name, fallback.Resolve(Json.Null)));
            }
        }
        store.Admin = auth.Admin;
        var subject = auth.User is { } user ? store.TripletOf(user) ?? 0 : 0;
        var exec = new Exec(store, settings, document, Json.Object(defaults), auth, subject);
        var mutation = selected.Kind == OperationKind.Mutation;
        try
        {
            if (mutation)
            {
                store.Begin();
            }
            GqlError? failure = null;
            Json? data = null;
            try
            {
                data = exec.Root(selected.Selections, mutation);
            }
            catch (GqlError error)
            {
                failure = error;
            }
            catch (Exception error)
            {
                failure = GqlError.Write(error.Message);
            }
            if (failure is not null)
            {
                if (mutation)
                {
                    store.Rollback();
                }
                return (failure.ToJson(), false);
            }
            if (mutation)
            {
                store.Commit();
            }
            return (Json.Object(("data", data!)), mutation);
        }
        catch (Exception error)
        {
            return (GqlError.Write(error.Message).ToJson(), false);
        }
        finally
        {
            store.Admin = false;
        }
    }

    /// <summary>The fields of the selections that apply to a type, without fragments.</summary>
    public List<Field> Flatten(List<Selection> selections, string typename)
    {
        var fields = new List<Field>();
        Collect(selections, typename, fields, 0);
        return fields;
    }

    private bool Included(List<Directive> directives)
    {
        foreach (var directive in directives)
        {
            var found = directive.Arguments.FindIndex(argument => argument.Name == "if");
            if (found < 0)
            {
                continue;
            }
            var condition = directive.Arguments[found].Value.Resolve(_variables);
            if (condition.Kind == JsonKind.Bool
                && ((directive.Name == "skip" && condition.BoolValue) || (directive.Name == "include" && !condition.BoolValue)))
            {
                return false;
            }
        }
        return true;
    }

    private void Collect(List<Selection> selections, string typename, List<Field> output, int depth)
    {
        if (depth > 64)
        {
            throw GqlError.Validation("fragments are nested too deeply", "$.query");
        }
        foreach (var selection in selections)
        {
            switch (selection)
            {
                case Field field:
                    if (Included(field.Directives) && !output.Any(seen => seen.ResponseKey == field.ResponseKey))
                    {
                        output.Add(field);
                    }
                    break;
                case Spread spread:
                    if (Included(spread.Directives))
                    {
                        if (!_document.Fragments.TryGetValue(spread.Name, out var fragment))
                        {
                            throw GqlError.Validation($"fragment '{spread.Name}' is undefined", "$.query");
                        }
                        Collect(fragment, typename, output, depth + 1);
                    }
                    break;
                case Inline inline:
                    if ((inline.On is null || inline.On == typename) && Included(inline.Directives))
                    {
                        Collect(inline.Selections, typename, output, depth + 1);
                    }
                    break;
            }
        }
    }

    private static KeyValuePair<string, Json> Pair(Field field, Json value) => new(field.ResponseKey, value);

    private Json Root(List<Selection> selections, bool mutation)
    {
        var typename = mutation ? "mutation_root" : "query_root";
        var data = new List<KeyValuePair<string, Json>>();
        foreach (var field in Flatten(selections, typename))
        {
            _memo.Clear();
            data.Add(Pair(field, RootField(field, typename, mutation)));
        }
        return Json.Object(data);
    }

    private Json RootField(Field field, string root, bool mutation)
    {
        var name = field.Name;
        if (name == "__typename")
        {
            return Json.Str(root);
        }
        if (mutation)
        {
            if (MutationField(field) is { } mutated)
            {
                return mutated;
            }
        }
        else
        {
            switch (name)
            {
                case "__schema":
                    return ProjectSchema(_settings.Introspection.Get("__schema") ?? Json.Null, field.Selections, "__Schema");
                case "__type":
                    var wanted = field.Argument("name", _variables)?.AsStr();
                    var types = _settings.Introspection.Get("__schema")?.Get("types")?.AsList() ?? [];
                    var found = types.FirstOrDefault(item => item.Get("name")?.AsStr() == wanted) ?? Json.Null;
                    return ProjectSchema(found, field.Selections, "__Type");
                case "guest":
                    return Guest(field);
                case "jwt":
                    return Jwt(field);
            }
            if (SelectField(field) is { } selected)
            {
                return selected;
            }
        }
        throw GqlError.Validation($"field '{name}' not found in type: '{root}'", $"$.selectionSet.{name}");
    }

    /// <summary>Selects from the recorded schema: its keys are the field names.</summary>
    private Json ProjectSchema(Json value, List<Selection> selections, string typename)
    {
        if (value.Kind == JsonKind.Array)
        {
            return Json.Array(value.Items.Select(item => ProjectSchema(item, selections, typename)).ToList());
        }
        if (value.Kind != JsonKind.Object)
        {
            return value;
        }
        var output = new List<KeyValuePair<string, Json>>();
        foreach (var field in Flatten(selections, typename))
        {
            Json found;
            if (field.Name == "__typename")
            {
                found = Json.Str(typename);
            }
            else
            {
                var inner = value.Get(field.Name) ?? Json.Null;
                var next = field.Name switch
                {
                    "queryType" or "mutationType" or "subscriptionType" or "types"
                        or "type" or "ofType" or "interfaces" or "possibleTypes" => "__Type",
                    "fields" => "__Field",
                    "args" or "inputFields" => "__InputValue",
                    "enumValues" => "__EnumValue",
                    "directives" => "__Directive",
                    _ => typename,
                };
                found = field.Selections.Count == 0 ? inner : ProjectSchema(inner, field.Selections, next);
            }
            output.Add(Pair(field, found));
        }
        return Json.Object(output);
    }

    private Json TokenResponse(Field field, string typename, long? id, string? error)
    {
        var token = id is { } subject ? Crypto.SignToken(_settings.Secret, unchecked((ulong)subject), Now()) : null;
        var output = new List<KeyValuePair<string, Json>>();
        foreach (var item in Flatten(field.Selections, typename))
        {
            var value = item.Name switch
            {
                "__typename" => Json.Str(typename),
                "token" => token is null ? Json.Null : Json.Str(token),
                "linkId" => id is { } link ? Json.Int(link) : Json.Null,
                "error" => error is null ? Json.Null : Json.Str(error),
                var other => throw GqlError.Validation(
                    $"field '{other}' not found in type: '{typename}'",
                    $"$.selectionSet.{field.Name}.{other}"),
            };
            output.Add(Pair(item, value));
        }
        return Json.Object(output);
    }

    /// <summary><c>guest</c>: a new link of type <c>User</c>, with a token for it. Who asks does not matter, so the layer below is asked as the server.</summary>
    private Json Guest(Field field)
    {
        _store.Begin();
        var admin = _store.Admin;
        _store.Admin = true;
        long id;
        try
        {
            id = CreateGuest();
        }
        catch (Exception error) when (error is not GqlError)
        {
            _store.Admin = admin;
            _store.Rollback();
            throw GqlError.Write(error.Message);
        }
        _store.Admin = admin;
        _store.Commit();
        return TokenResponse(field, "GuestOutput", id, null);
    }

    private long CreateGuest()
    {
        var user = CoreId("User");
        var id = _store.Reserve(1)[0];
        _store.CreateLink(0, id, user ?? id, 0, 0);
        // A guest of a database with `deep/users` is one of the users.
        if (CoreId("Join") is { } join && ContainedIn("deep", "users") is { } users)
        {
            _store.CreateLink(0, null, join, users, id);
        }
        return id;
    }

    /// <summary><c>jwt(input: {linkId})</c>: a token for the asker or, with permission, for the link named; no link names the asker, as a "who am I" query.</summary>
    private Json Jwt(Field field)
    {
        var asker = _auth.User;
        var named = field.Argument("input", _variables)?.Get("linkId")?.AsI64();
        if ((named ?? asker) is not { } target)
        {
            return TokenResponse(field, "JWTOutput", null, "No link and no subject.");
        }
        if (!(_auth.Admin || !_store.HasPolicy || asker == target))
        {
            return TokenResponse(field, "JWTOutput", null, "Forbidden.");
        }
        return _store.TripletOf(target) is null
            ? TokenResponse(field, "JWTOutput", null, $"No link {target}.")
            : TokenResponse(field, "JWTOutput", target, null);
    }

    /// <summary>The link a <c>Contain</c> named <paramref name="name"/> holds from the package named <paramref name="package"/>.</summary>
    public long? ContainedIn(string package, string name)
    {
        foreach (var (_, row) in _store.Values)
        {
            if (row.Kind == ValueKind.String && row.Value.AsStr() == name
                && _store.TripletOf(row.Owner) is { } triplet
                && RawRow(triplet) is { } link
                && _store.ValueOfOwner(ValueKind.String, link.FromId) is { } parent
                && parent.Value.Value.AsStr() == package)
            {
                return link.ToId;
            }
        }
        return null;
    }

    /// <summary>The id of a type of <c>@deep-foundation/core</c>.</summary>
    public long? CoreId(string name) => ContainedIn("@deep-foundation/core", name);
}
