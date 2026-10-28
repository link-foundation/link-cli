using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CausalProjectionTests
{
    private static string Author(int value) => value.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalPayload Row(string? name) => new(0, 2, name);
    private static string[] Wires(IEnumerable<CausalVersion> values) => values.Select(v => v.Encode()).ToArray();

    [Fact]
    public void IndependentProjectionVectorsRetainCompleteEvidenceBeforeVisibility()
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "causal-projection-v1.json")));
        var cases = document.RootElement.GetProperty("cases"); Assert.Equal(132, cases.GetArrayLength());
        foreach (var fixture in cases.EnumerateArray())
        {
            var values = fixture.GetProperty("input").EnumerateArray().Select(v => CausalVersion.Decode(JsonSerializer.Serialize(v))).ToArray();
            var policy = fixture.GetProperty("policy").GetString() == "reject" ? CausalConflictPolicy.Reject : CausalConflictPolicy.MaxDot;
            foreach (var input in new[] { values, values.Reverse().ToArray() })
            {
                var before = Wires(input);
                if (!fixture.GetProperty("valid").GetBoolean())
                {
                    var error = Assert.ThrowsAny<Exception>(() => new CausalProjection(input, policy));
                    Assert.Contains(fixture.GetProperty("error").GetString()!, error.Message, StringComparison.OrdinalIgnoreCase);
                }
                else
                {
                    var projection = new CausalProjection(input, policy);
                    Assert.Equal(fixture.GetProperty("retained").EnumerateArray().Select(v => JsonSerializer.Serialize(v)), Wires(projection.Versions()));
                    Assert.Equal(fixture.GetProperty("selected").EnumerateArray().Select(v => JsonSerializer.Serialize(v)), Wires(projection.Selected()));
                    var rows = projection.Rows(); var expected = fixture.GetProperty("rows").EnumerateArray().ToArray();
                    Assert.Equal(expected.Length, rows.Length);
                    for (int index = 0; index < rows.Length; index++)
                    {
                        Assert.Equal(expected[index][0].GetString(), rows[index].Link.Index.ToString(CultureInfo.InvariantCulture));
                        Assert.Equal(expected[index][1].GetString(), rows[index].Link.Source.ToString(CultureInfo.InvariantCulture));
                        Assert.Equal(expected[index][2].GetString(), rows[index].Link.Target.ToString(CultureInfo.InvariantCulture));
                        Assert.Equal(expected[index][3].GetString(), rows[index].Name);
                    }
                }
                Assert.Equal(before, Wires(input));
            }
        }
    }

    [Fact]
    public void ProjectionOwnsCallerAndReturnedClockCollections()
    {
        var live = CausalVersion.Advance(1, Author(1), 0, [], Row("\ufeff雪😀\0"));
        var deleted = CausalVersion.Advance(1, Author(2), 0, [], null);
        var input = new[] { live, deleted }; var expected = Wires(input);
        var projection = new CausalProjection(input, CausalConflictPolicy.MaxDot);
        ((CausalDot[])live.Clock)[0] = new(Author(1), 99); input[0] = deleted;
        Assert.Empty(projection.Rows()); Assert.Equal(expected, Wires(projection.Versions()));
        ((CausalDot[])projection.Versions()[0].Clock)[0] = new(Author(1), 99);
        ((CausalDot[])projection.Selected()[0].Clock)[0] = new(Author(2), 99);
        Assert.Equal(expected, Wires(projection.Versions()));
        var visible = new CausalProjection([CausalVersion.Advance(2, Author(3), 0, [], Row(""))], CausalConflictPolicy.Reject);
        visible.Rows()[0] = visible.Rows()[0] with { Name = "returned" };
        Assert.Equal("", visible.Rows()[0].Name);
    }

    [Fact]
    public void CompleteInputBoundsPrecedeDeduplicationAndDominance()
    {
        var value = CausalVersion.Advance(1, Author(1), 0, [], Row("row"));
        Assert.Throws<InvalidDataException>(() => new CausalProjection([], (CausalConflictPolicy)99));
        foreach (var options in new[] { new CausalProjectionOptions(0), new(4097), new(1, 0), new(1, 8_388_609) })
            Assert.Throws<InvalidDataException>(() => new CausalProjection([], CausalConflictPolicy.Reject, options));
        Assert.ThrowsAny<Exception>(() => new CausalProjection(null!, CausalConflictPolicy.Reject));
        Assert.Throws<InvalidDataException>(() => new CausalProjection([null!], CausalConflictPolicy.Reject));
        Assert.Throws<InvalidDataException>(() => new CausalProjection([value, value], CausalConflictPolicy.Reject, new(1)));
        Assert.Equal(new[] { value.Encode() }, Wires(new CausalProjection([value, value], CausalConflictPolicy.Reject).Versions()));
        Assert.Throws<InvalidDataException>(() => new CausalProjection([value], CausalConflictPolicy.Reject, new(MaxBytes: value.Encode().Length - 1)));
        Assert.Single(new CausalProjection([value], CausalConflictPolicy.Reject, new(MaxBytes: value.Encode().Length)).Rows());
        Assert.Throws<InvalidDataException>(() => new CausalProjection(Enumerable.Repeat(value, 65).ToArray(), CausalConflictPolicy.MaxDot));
        var concurrent = Enumerable.Range(1, 64).Select(i => CausalVersion.Advance(1, Author(i), 0, [], null)).ToArray();
        Assert.Equal(64, new CausalProjection(concurrent, CausalConflictPolicy.MaxDot).Versions().Length);
        Assert.Empty(new CausalProjection([], CausalConflictPolicy.Reject, new(1, 1)).Rows());
        var newer = CausalVersion.Advance(1, Author(1), 1, [value], null);
        Assert.Throws<InvalidDataException>(() => new CausalProjection([value, newer], CausalConflictPolicy.MaxDot, new(1)));
    }

    [Fact]
    public void NumericOrderAndFullUnsignedValuesPreserveNullNames()
    {
        var input = new[] { ulong.MaxValue, 10UL, 2UL }.Select((address, index) =>
            CausalVersion.Advance(address, Author(index + 1), ulong.MaxValue - 1, [], new(ulong.MaxValue, 0, null))).ToArray();
        var projection = new CausalProjection(input, CausalConflictPolicy.Reject);
        Assert.Equal(new[] { 2UL, 10UL, ulong.MaxValue }, projection.Rows().Select(v => v.Link.Index));
        Assert.Equal(ulong.MaxValue, projection.Rows()[0].Link.Source); Assert.Null(projection.Rows()[0].Name);
        Assert.Equal(ulong.MaxValue, projection.Selected()[0].Dot.Counter);
    }

    [Fact]
    public void FiniteAdmissionAccepts4096VersionsAndExactlyEightMiB()
    {
        static CausalVersion Make(int index, string name = "", ulong source = 0) =>
            CausalVersion.Advance((ulong)index, Author(1), (ulong)index - 1, [], new(source, 0, name));
        var many = Enumerable.Range(1, 4096).Select(i => Make(i)).ToArray();
        Assert.Equal(4096, new CausalProjection(many, CausalConflictPolicy.Reject).Rows().Length);
        Assert.Throws<InvalidDataException>(() => new CausalProjection([.. many, Make(4097)], CausalConflictPolicy.Reject));
        const int budget = 8_388_608; var wide = new List<CausalVersion>(); int bytes = 0;
        while (true)
        {
            var next = Make(wide.Count + 1, new('a', 4096)); int size = next.Encode().Length;
            if (bytes + size + 256 > budget) break;
            wide.Add(next); bytes += size;
        }
        var last = Make(wide.Count + 1); int rest = budget - bytes - last.Encode().Length;
        if (rest % 2 != 0) { last = Make(wide.Count + 1, "", 10); rest = budget - bytes - last.Encode().Length; }
        Assert.InRange(rest, 0, 8192); Assert.Equal(0, rest % 2);
        wide.Add(last with { Payload = last.Payload! with { Name = new('a', rest / 2) } });
        Assert.Equal(budget, wide.Sum(v => v.Encode().Length));
        Assert.Equal(wide.Count, new CausalProjection(wide, CausalConflictPolicy.Reject).Rows().Length);
        Assert.Throws<InvalidDataException>(() => new CausalProjection([.. wide, Make(wide.Count + 1)], CausalConflictPolicy.Reject));
    }
}
