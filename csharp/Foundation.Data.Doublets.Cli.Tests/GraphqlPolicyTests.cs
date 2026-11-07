using System.Text;
using Foundation.Data.Doublets.Cli.Graphql;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>The GraphQL executor under a permission policy, run inside this test binary. The Rust port has the same cases in <c>graphql_policy.rs</c>.</summary>
public class GraphqlPolicyTests
{
    private static readonly Auth Admin = new(null, true);
    private static readonly Auth Nobody = new(null, false);

    private static Auth User(long id) => new(id, false);

    /// <summary>Runs <paramref name="body"/> over a store under the policy of <paramref name="rule"/>, with Deep's core loaded.</summary>
    private static async Task WithStore(string rule, Action<Func<string, Auth, string>, Func<long>> body)
    {
        var database = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        try
        {
            var query = $"() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (Anything: Anything Anything) (Doc: Type Doc) {rule})";
            Clink.AssertSucceeded(await Clink.RunAsync("--db", database, "--auto-create-missing-references", query));
            using var links = new NamedTypesDecorator<uint>(database, false);
            var store = new DeepStore(links, new PermissionPolicy<uint>(links, links));
            store.LoadPackage(Json.Parse(GraphqlTests.Vectors("deep-core.json")));
            var settings = new Settings(Encoding.UTF8.GetBytes("secret"), null);
            string Ask(string text, Auth auth) => Exec.Execute(store, settings, text, null, Json.Null, auth).Response.Write();
            long Guest() => Exec.Execute(store, settings, "{guest{linkId}}", null, Json.Null, Nobody).Response
                .Get("data")!.Get("guest")!.Get("linkId")!.AsI64()!.Value;
            body(Ask, Guest);
        }
        finally
        {
            foreach (var file in Directory.GetFiles(Path.GetDirectoryName(database)!, Path.GetFileName(database) + "*"))
            {
                File.Delete(file);
            }
        }
    }

    [Fact]
    public Task WhatASubjectCreatedIsOnlyItsOwn() => WithStore("(AllowWrite (Anything Doc))", (ask, guest) =>
    {
        var (first, second) = (guest(), guest());
        Assert.NotEqual(first, second);
        Assert.Equal(
            "{\"data\":{\"insert_links_one\":{\"id\":9000}}}",
            ask("mutation{insert_links_one(object:{id:9000,type_id:9000,string:{data:{value:\"mine\"}}}){id}}", User(first)));

        const string Links = "{links(where:{id:{_eq:9000}}){id string{value}}}";
        const string Seen = "{\"data\":{\"links\":[{\"id\":9000,\"string\":{\"value\":\"mine\"}}]}}";
        const string Unseen = "{\"data\":{\"links\":[]}}";
        Assert.Equal(Seen, ask(Links, User(first)));
        Assert.Equal(Unseen, ask(Links, User(second)));
        Assert.Equal(Unseen, ask(Links, Nobody));
        Assert.Equal(Seen, ask(Links, Admin));

        // A value is seen with its owner.
        const string Values = "{strings(where:{value:{_eq:\"mine\"}}){value}}";
        Assert.Equal("{\"data\":{\"strings\":[{\"value\":\"mine\"}]}}", ask(Values, User(first)));
        Assert.Equal("{\"data\":{\"strings\":[]}}", ask(Values, User(second)));
    });

    [Fact]
    public Task WritingWhatIsNotYoursIsForbidden() => WithStore("(AllowRead (Anything Anything))", (ask, guest) =>
    {
        ask("mutation{insert_links_one(object:{id:9000,type_id:1,number:{data:{value:1}}}){id}}", Admin);
        var visitor = User(guest());
        foreach (var mutation in new[]
        {
            "mutation{insert_links_one(object:{type_id:9000}){id}}",
            "mutation{update_links_by_pk(id:9000,_set:{to_id:1}){id}}",
            "mutation{delete_links_by_pk(id:9000){id}}",
            "mutation{update_numbers(where:{link_id:{_eq:9000}},_set:{value:2}){affected_rows}}",
            "mutation{delete_numbers(where:{link_id:{_eq:9000}}){affected_rows}}",
            "mutation{insert_strings_one(object:{link_id:9000,value:\"x\"}){id}}",
        })
        {
            Assert.Contains("\"code\":\"permission-error\"", ask(mutation, visitor));
        }
        // Nothing changed, and nothing was left half done.
        Assert.Equal(
            "{\"data\":{\"links\":[{\"to_id\":0,\"number\":{\"value\":1}}]}}",
            ask("{links(where:{id:{_eq:9000}}){to_id number{value}}}", visitor));
    });

    [Fact]
    public Task ATokenForAnotherLinkNeedsPermission() => WithStore("(AllowRead (Anything Anything))", (ask, guest) =>
    {
        var (first, second) = (guest(), guest());
        static string Of(long id) => $"{{jwt(input:{{linkId:{id}}}){{linkId error}}}}";
        const string Forbidden = "{\"data\":{\"jwt\":{\"linkId\":null,\"error\":\"Forbidden.\"}}}";
        Assert.Equal(Forbidden, ask(Of(second), User(first)));
        Assert.Equal(Forbidden, ask(Of(second), Nobody));
        var own = $"{{\"data\":{{\"jwt\":{{\"linkId\":{first},\"error\":null}}}}}}";
        Assert.Equal(own, ask(Of(first), User(first)));
        Assert.Equal(own, ask(Of(first), Admin));
        // No link names the asker.
        Assert.Equal(own, ask("{jwt{linkId error}}", User(first)));
    });

    [Fact]
    public Task CanListsTheGrantsInDeepTerms() => WithStore("(AllowRead (Anything Anything))", (ask, _) =>
    {
        // AllowSelect of the loaded core, and 0 for `Anything` on both sides.
        Assert.Equal(
            "{\"data\":{\"can\":[{\"rule_id\":0,\"action_id\":28,\"object_id\":0,\"subject_id\":0,\"action\":{\"id\":28}}]}}",
            ask("{can{rule_id action_id object_id subject_id action{id}}}", Admin));
        Assert.Equal(
            "{\"data\":{\"links\":[{\"can_action\":[{\"subject_id\":0}],\"can_object\":[],\"can_rule\":[],\"can_subject\":[]}]}}",
            ask("{links(where:{id:{_eq:28}}){can_action{subject_id} can_object{rule_id} can_rule{rule_id} can_subject{rule_id}}}", Admin));
    });

    [Fact]
    public void AuthorizationReadsTheSecretAndTheToken()
    {
        var secret = Encoding.UTF8.GetBytes("secret");
        var settings = new Settings(secret, "root");
        var open = new Settings(secret, null);
        var token = Crypto.SignToken(secret, 7, 0);

        Assert.Equal(Nobody, settings.Authorize(null, null));
        Assert.Equal(Nobody, settings.Authorize("  ", null));
        Assert.Equal(Nobody, settings.Authorize("Bearer ", null));
        Assert.Equal(User(7), settings.Authorize($"Bearer {token}", null));
        Assert.Equal(User(7), settings.Authorize(token, null));
        Assert.Equal(new Auth(7, true), settings.Authorize($"Bearer {token}", "root"));
        Assert.Equal("invalid x-hasura-admin-secret", Assert.Throws<FormatException>(() => settings.Authorize(null, "wrong")).Message);
        Assert.Equal("invalid x-hasura-admin-secret", Assert.Throws<FormatException>(() => open.Authorize(null, "root")).Message);
        Assert.StartsWith("Could not verify JWT", Assert.Throws<FormatException>(() => settings.Authorize("a.b.c", null)).Message);

        var (a, b) = (Settings.RandomSecret(), Settings.RandomSecret());
        Assert.Equal(32, a.Length);
        Assert.NotEqual(a, b);
    }

    private static string TokenWith(byte[] secret, string header, string claims)
    {
        var signed = Crypto.Base64Url(Encoding.UTF8.GetBytes(header)) + "." + Crypto.Base64Url(Encoding.UTF8.GetBytes(claims));
        return signed + "." + Crypto.Base64Url(System.Security.Cryptography.HMACSHA256.HashData(secret, Encoding.UTF8.GetBytes(signed)));
    }

    [Fact]
    public void TokensAreRefusedForTheirAlgorithmExpiryAndSubject()
    {
        const string Hs256 = "{\"alg\":\"HS256\"}";
        var key = Encoding.UTF8.GetBytes("k");
        void Refused(string token, string message) =>
            Assert.Contains(message, Assert.Throws<FormatException>(() => Crypto.VerifyToken(key, token, 100)).Message);
        Refused(TokenWith(key, "{\"alg\":\"none\"}", "{}"), "only HS256");
        Refused(TokenWith(key, Hs256, "{\"exp\":100}"), "JWTExpired");
        Refused(TokenWith(key, Hs256, "{\"exp\":101}"), "no x-hasura-user-id");
        Refused(TokenWith(key, Hs256, "{\"https://hasura.io/jwt/claims\":{\"x-hasura-user-id\":\"not a number\"}}"), "no x-hasura-user-id");
        Refused(TokenWith(Encoding.UTF8.GetBytes("other"), Hs256, "{}"), "JWSInvalidSignature");
        Refused("a.b", "malformed");
    }
}
