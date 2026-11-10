using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class JournalValueContractTests
{
    private static Transition<ulong> Row => new(Guid.Parse("00000000-0000-0000-0000-000000000001"),
        long.MaxValue, DateTimeOffset.FromUnixTimeMilliseconds(1730000000000), TransitionKind.Update,
        new Link<ulong>(ulong.MaxValue, 11, 12), new Link<ulong>(ulong.MaxValue, 21, 22));

    [Fact]
    public void CapturedNamesOwnTheirValuesAndExposeAReadOnlyList()
    {
        var change = new NameChange<ulong>(ulong.MaxValue, "before 雪\0", "after=😀\n");
        var source = new List<NameChange<ulong>> { change };
        var record = new JournalRecord<ulong>(Row, names: source);
        var wire = record.Serialize(); var hash = record.GetHashCode();
        source[0] = new(1, null, "changed"); source.Clear();
        Assert.Equal(new[] { change }, record.Names);
        Assert.Equal(wire, record.Serialize()); Assert.Equal(hash, record.GetHashCode());
        var exposed = Assert.IsAssignableFrom<IList<NameChange<ulong>>>(record.Names);
        Assert.True(exposed.IsReadOnly);
        Assert.Throws<NotSupportedException>(() => exposed[0] = new(2, null, null));
        Assert.Throws<NotSupportedException>(() => exposed.Add(change));
        Assert.Equal(new[] { change }, record.Names);
        Assert.True(JournalRecord<ulong>.TryParse(wire, out var restored));
        Assert.Equal(record, restored); Assert.Equal(hash, restored.GetHashCode());
    }

    [Fact]
    public void CaptureEnumeratesOnceAndDoesNotReadTheSourceAgain()
    {
        var enumerations = 0;
        var original = new NameChange<ulong>(ulong.MaxValue, null, "captured");
        var current = original;
        IEnumerable<NameChange<ulong>> Source()
        {
            ++enumerations; yield return current;
        }
        var record = new JournalRecord<ulong>(Row, names: Source());
        current = new(ulong.MaxValue, "captured", "changed");
        Assert.Equal(1, enumerations); Assert.Equal(new[] { original }, record.Names);
        var same = new JournalRecord<ulong>(Row, names: new[] { original });
        Assert.True(record.Equals(same)); Assert.Equal(record.GetHashCode(), same.GetHashCode());
        Assert.Equal(record.Serialize(), same.Serialize()); Assert.Equal(1, enumerations);
    }

    [Fact]
    public void EqualityAndHashCollectionsIncludeTheCompleteOrderedPayload()
    {
        var names = new[] { new NameChange<ulong>(1, null, "first"), new NameChange<ulong>(2, "", "second") };
        var transition = Row with { Before = default, After = default };
        var record = new JournalRecord<ulong>(transition, true, names);
        var equal = new JournalRecord<ulong>(transition, true, names.ToList());
        Assert.True(record.Equals(record)); Assert.True(record.Equals(equal)); Assert.True(equal.Equals(record));
        Assert.True(record.Equals((object)equal)); Assert.False(record.Equals((object?)null));
        Assert.False(record.Equals((JournalRecord<ulong>?)null)); Assert.False(record.Equals("unrelated"));
        Assert.Equal(record.GetHashCode(), equal.GetHashCode());
        var distinct = new[] {
            new JournalRecord<ulong>(transition, false, names),
            new JournalRecord<ulong>(transition, true, names.Reverse()),
            new JournalRecord<ulong>(transition, true, names.Take(1)),
            new JournalRecord<ulong>(transition, true, new[] { names[0], names[1] with { After = "different" } }),
            new JournalRecord<ulong>(transition with { TransactionId = Guid.Empty }, true, names),
            new JournalRecord<ulong>(transition with { Sequence = long.MinValue }, true, names),
            new JournalRecord<ulong>(transition with { Timestamp = transition.Timestamp.AddMilliseconds(1) }, true, names),
            new JournalRecord<ulong>(transition with { Kind = TransitionKind.Delete }, true, names),
            new JournalRecord<ulong>(transition with { Before = Row.Before }, true, names),
            new JournalRecord<ulong>(transition with { After = Row.After }, true, names),
        };
        var set = new HashSet<JournalRecord<ulong>> { record };
        Assert.False(set.Add(equal));
        foreach (var other in distinct) { Assert.False(record.Equals(other)); Assert.True(set.Add(other)); }
        Assert.Equal(distinct.Length + 1, set.Count);
    }

    [Fact]
    public void HeaderPropertiesAndEquivalentInstantsPreserveTransitionValues()
    {
        var record = new JournalRecord<ulong>(Row);
        Assert.Equal(Row, record.Transition); Assert.Equal(Row.TransactionId, record.TransactionId);
        Assert.Equal(Row.Sequence, record.Sequence); Assert.Equal(Row.Timestamp, record.Timestamp);
        Assert.Equal(Row.Kind, record.Kind); Assert.Equal(Row.Before, record.Before); Assert.Equal(Row.After, record.After);
        Assert.False(record.NameOnly); Assert.Empty(record.Names);
        var same = new JournalRecord<ulong>(Row with { Timestamp = Row.Timestamp.ToOffset(TimeSpan.FromMinutes(330)) }, names: []);
        Assert.Equal(record, same); Assert.Equal(record.GetHashCode(), same.GetHashCode());
        Assert.Equal(record.Serialize(), same.Serialize());
        Assert.True(JournalRecord<ulong>.TryParse(record.Serialize(), out var restored));
        Assert.Equal(record, restored);
    }

    [Fact]
    public void PublicCodecPreservesEveryUtf16UnitAndDistinguishesNullFromEmpty()
    {
        // A finite 65,536-unit name covers the full existing .NET string domain,
        // including lone surrogate units; it does not claim UTF-8 interoperability.
        var allUnits = new string(Enumerable.Range(0, 65536).Select(value => (char)value).ToArray());
        foreach (var before in new string?[] { null, "" })
        {
            var record = new JournalRecord<ulong>(Row, names: new[] { new NameChange<ulong>(ulong.MaxValue, before, allUnits) });
            var wire = record.Serialize(); Assert.True(JournalRecord<ulong>.TryParse(wire, out var restored));
            Assert.Equal(record, restored); Assert.Equal(before, restored.Names[0].Before);
            Assert.Equal(allUnits, restored.Names[0].After); Assert.Equal(wire, restored.Serialize());
        }
    }

    [Theory]
    [InlineData("0")]
    [InlineData("000")]
    [InlineData("00000")]
    [InlineData("GGGG")]
    [InlineData("00+1")]
    [InlineData(" 001")]
    [InlineData("001 ")]
    [InlineData("0x01")]
    [InlineData("-001")]
    [InlineData("+001")]
    [InlineData("１２３４")]
    [InlineData("００００")]
    public void MalformedNameUnitsAreRejectedWithoutAResult(string unit)
    {
        var wire = "v2" + Row.Serialize()[2..] + "|r|18446744073709551615,-," + unit;
        Assert.False(JournalRecord<ulong>.TryParse(wire, out var result)); Assert.Null(result);
    }
}
