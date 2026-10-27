using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class CausalVersionTests
{
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(
        Path.Combine(AppContext.BaseDirectory, "Protocol", "causal-version-v1.json")));
    private static CausalVersion FromArray(JsonElement value) => CausalVersion.Decode(JsonSerializer.Serialize(value));
    private static string[] Wires(IEnumerable<CausalVersion> values) => values.Select(value => value.Encode()).ToArray();

    [Fact]
    public void IndependentCodecVectorsPreserveCanonicalBytesAndRejectMalformedInputs()
    {
        using var vectors = Vectors();
        foreach (var fixture in vectors.RootElement.GetProperty("cases").EnumerateArray())
        {
            string wire = fixture.GetProperty("record").GetString()!;
            if (fixture.GetProperty("valid").GetBoolean())
                Assert.Equal(fixture.GetProperty("canonical").GetString(), CausalVersion.Decode(wire).Encode());
            else Assert.ThrowsAny<Exception>(() => CausalVersion.Decode(wire));
        }
    }

    [Fact]
    public void MergeRetainsConcurrentSiblingsAndRejectsContradictionsBeforePruning()
    {
        using var vectors = Vectors();
        foreach (var fixture in vectors.RootElement.GetProperty("merges").EnumerateArray())
        {
            var left = fixture.GetProperty("left").EnumerateArray().Select(FromArray).ToArray();
            var right = fixture.GetProperty("right").EnumerateArray().Select(FromArray).ToArray();
            var before = Wires(left.Concat(right));
            if (fixture.TryGetProperty("valid", out var valid) && !valid.GetBoolean())
                Assert.Throws<InvalidDataException>(() => CausalVersion.Merge(left, right));
            else
            {
                var expected = fixture.GetProperty("expected").EnumerateArray().Select(FromArray).ToArray();
                Assert.Equal(Wires(expected), Wires(CausalVersion.Merge(left, right)));
                Assert.Equal(Wires(expected), Wires(CausalVersion.Merge(right, left)));
            }
            Assert.Equal(before, Wires(left.Concat(right)));
        }
    }

    [Fact]
    public void BoundedIndependentSchedulesConvergeAndSatisfyMergeLaws()
    {
        using var vectors = Vectors();
        var events = vectors.RootElement.GetProperty("events").EnumerateArray().Select(FromArray).ToArray();
        int schedules = 0;
        foreach (int a in Enumerable.Range(0, 4)) foreach (int b in Enumerable.Range(0, 4))
            foreach (int c in Enumerable.Range(0, 4)) foreach (int d in Enumerable.Range(0, 4))
            {
                int[] order = [a, b, c, d]; if (order.Distinct().Count() != 4) continue;
                CausalVersion[] state = [];
                foreach (int index in order) state = CausalVersion.Merge(state, [events[index]]);
                Assert.Equal(new[] { events[3].Encode() }, Wires(state)); schedules++;
            }
        Assert.Equal(24, schedules);
        CausalVersion[][] states = [[], [events[0]], [events[1]], [events[2]], [events[3]],
            CausalVersion.Merge([events[1]], [events[2]])];
        foreach (var a in states) foreach (var b in states) foreach (var c in states)
        {
            Assert.Equal(Wires(a), Wires(CausalVersion.Merge(a, a)));
            Assert.Equal(Wires(CausalVersion.Merge(a, b)), Wires(CausalVersion.Merge(b, a)));
            Assert.Equal(Wires(CausalVersion.Merge(CausalVersion.Merge(a, b), c)),
                Wires(CausalVersion.Merge(a, CausalVersion.Merge(b, c))));
        }
    }

    [Fact]
    public void AdvancementJoinsObservedClocksWithoutWrappingAndOwnsResults()
    {
        using var vectors = Vectors();
        foreach (var fixture in vectors.RootElement.GetProperty("advances").EnumerateArray())
        {
            var observed = fixture.GetProperty("observed").EnumerateArray().Select(FromArray).ToArray();
            var before = Wires(observed); var input = fixture.GetProperty("payload");
            CausalPayload? row = input.ValueKind == JsonValueKind.Null ? null : new(
                ulong.Parse(input[0].GetString()!, System.Globalization.CultureInfo.InvariantCulture),
                ulong.Parse(input[1].GetString()!, System.Globalization.CultureInfo.InvariantCulture),
                input[2].ValueKind == JsonValueKind.Null ? null : System.Text.Encoding.UTF8.GetString(Convert.FromHexString(input[2].GetString()!)));
            CausalVersion Apply() => CausalVersion.Advance(
                ulong.Parse(fixture.GetProperty("address").GetString()!, System.Globalization.CultureInfo.InvariantCulture),
                fixture.GetProperty("author").GetString()!,
                ulong.Parse(fixture.GetProperty("lastCounter").GetString()!, System.Globalization.CultureInfo.InvariantCulture), observed, row);
            if (fixture.TryGetProperty("valid", out var valid) && !valid.GetBoolean())
                Assert.Throws<InvalidDataException>(() => Apply());
            else Assert.Equal(FromArray(fixture.GetProperty("expected")).Encode(), Apply().Encode());
            Assert.Equal(before, Wires(observed));
        }
        var events = vectors.RootElement.GetProperty("events").EnumerateArray().Select(FromArray).ToArray();
        string author = vectors.RootElement.GetProperty("authors")[0].GetString()!;
        var siblings = CausalVersion.Merge([events[1]], [events[2]]);
        var next = CausalVersion.Advance(1, author, 2, siblings, new(4, 5, "resolved"));
        Assert.Equal(events[3].Encode(), next.Encode());
        Assert.Equal(1ul, CausalVersion.Advance(1, author, 0, [], null).Dot.Counter);
        Assert.Equal(3ul, CausalVersion.Advance(1, author, 0, [events[1]], null).Dot.Counter);
        Assert.Throws<InvalidDataException>(() => CausalVersion.Advance(1, author, ulong.MaxValue, siblings, null));
        Assert.Throws<InvalidDataException>(() => CausalVersion.Advance(2, author, 0, siblings, null));
        var merged = CausalVersion.Merge([events[0]], []);
        ((CausalDot[])merged[0].Clock)[0] = new(author, 9);
        Assert.Equal(1ul, events[0].Clock[0].Counter);
        Assert.Equal(events[3].Encode(), CausalVersion.Merge([next], siblings).Single().Encode());
    }

    [Fact]
    public void PublicStateAndRetainedSiblingBoundsAreValidated()
    {
        const string author = "00000000000000000000000000000001";
        var value = new CausalVersion(1, new(author, 1), [new(author, 1)], new(0, 0, null));
        foreach (var invalid in new[] { value with { Address = 0 }, value with { Dot = null! },
            value with { Clock = null! }, value with { Clock = [] },
            value with { Payload = new(0, 0, "\ud800") },
            value with { Payload = new(0, 0, new string('雪', 1366)) } })
            Assert.ThrowsAny<Exception>(() => invalid.Encode());
        Assert.Throws<ArgumentNullException>(() => CausalVersion.Merge(null!, []));
        Assert.Throws<InvalidDataException>(() => CausalVersion.Merge(Enumerable.Repeat(value, 65).ToArray(), []));
        var siblings = Enumerable.Range(1, 65).Select(n =>
        {
            string id = n.ToString("x32", System.Globalization.CultureInfo.InvariantCulture);
            return new CausalVersion(1, new(id, 1), [new(id, 1)], null);
        }).ToArray();
        Assert.Throws<InvalidDataException>(() => CausalVersion.Merge(siblings[..64], siblings[64..]));
    }
}
