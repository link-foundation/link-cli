using Foundation.Data.Doublets.Cli.Graphql;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>The JSON reader and writer of the GraphQL server, at their edges. The Rust port has the same cases in <c>graphql_json.rs</c>.</summary>
public class GraphqlJsonTests
{
    private static void RoundTrip(string text, string written) => Assert.Equal(written, Json.Parse(text).Write());

    [Theory]
    [InlineData("", "Invalid JSON at byte 0")]
    [InlineData("1 2", "Unexpected text after JSON at byte 2")]
    [InlineData("nul", "Invalid JSON at byte 0")]
    [InlineData("tru", "Invalid JSON at byte 0")]
    [InlineData("fals", "Invalid JSON at byte 0")]
    [InlineData("?", "Invalid JSON at byte 0")]
    [InlineData("[1", "Invalid JSON array at byte 2")]
    [InlineData("[1 2]", "Invalid JSON array at byte 3")]
    [InlineData("{1:2}", "Invalid JSON object at byte 1")]
    [InlineData("{\"a\" 1}", "Invalid JSON object at byte 5")]
    [InlineData("{\"a\":1", "Invalid JSON object at byte 6")]
    [InlineData("{\"a\":1 \"b\":2}", "Invalid JSON object at byte 7")]
    [InlineData("{\"a\":1,}", "Invalid JSON object at byte 7")]
    [InlineData("1.2.3", "Invalid JSON number at byte 0")]
    [InlineData("-", "Invalid JSON number at byte 0")]
    [InlineData("\"a", "Unterminated JSON string")]
    [InlineData("\"a\\", "Unterminated JSON string")]
    [InlineData("\"\\q\"", "Invalid JSON escape")]
    [InlineData("\"\\u12\"", "Invalid JSON escape")]
    [InlineData("\"\\uzzzz\"", "Invalid JSON escape")]
    [InlineData("\"\\ud83d\\uzzzz\"", "Invalid JSON escape")]
    public void MalformedTextIsRefused(string text, string message) =>
        Assert.Contains(message, Assert.Throws<FormatException>(() => Json.Parse(text)).Message);

    [Fact]
    public void NestingHasALimit()
    {
        Json.Parse(new string('[', 128) + new string(']', 128));
        foreach (var beyond in new[]
        {
            new string('[', 130) + new string(']', 130),
            string.Concat(Enumerable.Repeat("{\"a\":", 130)) + "1" + new string('}', 130),
        })
        {
            Assert.Contains("nested too deeply", Assert.Throws<FormatException>(() => Json.Parse(beyond)).Message);
        }
    }

    [Fact]
    public void ValuesAndEscapesSurviveARoundTrip()
    {
        RoundTrip(" [ ] ", "[]");
        RoundTrip(" { } ", "{}");
        RoundTrip("[null,true,false]", "[null,true,false]");
        RoundTrip("{\"b\":1,\"a\":[2]}", "{\"b\":1,\"a\":[2]}");
        RoundTrip("1.5", "1.5");
        RoundTrip("-2", "-2");
        RoundTrip("\"\\\"\\\\\\/\\n\\r\\t\\b\\f\\u0041\"", "\"\\\"\\\\/\\n\\r\\t\\u0008\\u000cA\"");
        RoundTrip("\"\\u0001\"", "\"\\u0001\"");
        RoundTrip("\"\\ud83d\\ude00\"", "\"😀\"");
        // A lone surrogate has no character of its own.
        RoundTrip("\"\\ud83d\"", "\"\uFFFD\"");
        RoundTrip("\"\\ude00\"", "\"\uFFFD\"");
    }

    [Fact]
    public void NumbersReadAsWhatTheyAre()
    {
        var whole = Json.Float(3.0);
        Assert.Equal(3, whole.AsI64());
        Assert.Equal(3.0, whole.AsF64());
        Assert.Null(Json.Float(3.5).AsI64());
        Assert.Equal(3.0, Json.Int(3).AsF64());
        Assert.Null(Json.Null.AsF64());
        Assert.Null(Json.Null.AsI64());
        Assert.Null(Json.Int(1).AsStr());
        Assert.Null(Json.Int(1).Get("a"));
        Assert.Single(Json.Int(1).AsList());
        Assert.Empty(Json.Null.AsList());
        Assert.Equal("1.5", Json.Float(1.5).Write());
        Assert.Equal("null", Json.Float(double.NaN).Write());
        Assert.Equal("null", Json.Float(double.PositiveInfinity).Write());
    }

    [Fact]
    public void ControlCharactersAreEscapedWhenWritten()
    {
        Assert.Equal("\"\\u0000\\u001f\\\"\\\\\\n\"", Json.Str("\0\u001f\"\\\n").Write());
        Assert.Equal(1e20, Json.Parse("99999999999999999999").AsF64());
    }
}
