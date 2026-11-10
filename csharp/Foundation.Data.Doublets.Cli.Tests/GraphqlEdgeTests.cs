using System.Security.Cryptography;
using System.Text;
using Foundation.Data.Doublets.Cli.Graphql;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>The refusals and rare shapes of the GraphQL parts that take text from outside.</summary>
public sealed class GraphqlEdgeTests
{
    private static uint? Same(Position position, long id) => (uint)id;

    private static string Planned(string where) => Planner.Compile(Json.Parse(where), Same).Plan.Render();

    [Fact]
    public void TextOutsideTheBase64UrlAlphabetIsRefused() =>
        Assert.Throws<FormatException>(() => Crypto.Base64UrlDecode("a!"));

    [Fact]
    public void ClaimsThatAreNotUtf8AreRefused()
    {
        var secret = Encoding.UTF8.GetBytes("secret");
        var header = Crypto.Base64Url(Encoding.UTF8.GetBytes("{\"alg\":\"HS256\"}"));
        var claims = Crypto.Base64Url([0xFF, 0xFE]);
        var signature = Crypto.Base64Url(HMACSHA256.HashData(secret, Encoding.UTF8.GetBytes(header + "." + claims)));

        Assert.Throws<FormatException>(() => Crypto.VerifyToken(secret, $"{header}.{claims}.{signature}", 1));
    }

    [Theory]
    [InlineData("{ a(b c) }", "Expected ':', found Some(Name(\"c\")).")]
    [InlineData("{ 1 }", "Expected a name, found Some(Int(1)).")]
    [InlineData("{ 1.5 }", "Expected a name, found Some(Float(1.5)).")]
    [InlineData("{ 1e2 }", "Expected a name, found Some(Float(100.0)).")]
    public void ASyntaxErrorNamesTheTokenItFound(string text, string message) =>
        Assert.Equal(message, Assert.Throws<FormatException>(() => Document.Parse(text)).Message);

    [Fact]
    public void AnIntersectionRendersItsParts() =>
        Assert.Equal("(intersect (ids 1) (ids 2))", Plan.Intersect([Plan.Ids([1]), Plan.Ids([2])]).Render());

    [Fact]
    public void AnIdListWithAnythingButNumbersIsNotAPlanOfIds() =>
        Assert.DoesNotContain("(ids", Planned("{\"id\":{\"_in\":[1,\"two\"]}}"));

    [Fact]
    public void TwoValuesForOnePositionMatchNothing() =>
        Assert.Equal("empty", Planned("{\"from_id\":{\"_eq\":1},\"_and\":[{\"from_id\":{\"_eq\":2}}]}"));

    [Fact]
    public void ANestedConditionThatIsNotAnObjectIsLeftToTheFilter() =>
        Assert.False(Planner.Compile(Json.Parse("{\"from\":5}"), Same).Exact);
}
