using System.Collections;
using System.Reflection;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EnumerableDeconstructionTests
{
    public static IEnumerable<object[]> PrefixCases() =>
        from arity in Enumerable.Range(1, 8)
        from length in Enumerable.Range(0, 10)
        select new object[] { arity, length };

    private sealed class SinglePassSequence(int length, int failAt = -1) : IEnumerable<int>
    {
        public int Enumerations { get; private set; }
        public int Reads { get; private set; }
        public bool Disposed { get; private set; }
        public IEnumerator<int> GetEnumerator()
        {
            Assert.Equal(1, ++Enumerations);
            return Iterate().GetEnumerator();
        }
        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        private IEnumerable<int> Iterate()
        {
            try
            {
                for (var i = 0; i < length; i++)
                {
                    if (i == failAt) throw new IOException("injected enumeration failure");
                    Reads++;
                    yield return i + 11;
                }
            }
            finally { Disposed = true; }
        }
    }

    private static object?[] Deconstruct(SinglePassSequence source, int arity)
    {
        var method = typeof(EnumerableExtensions).GetMethods(BindingFlags.Public | BindingFlags.Static)
            .Single(m => m.Name == "Deconstruct" && m.GetParameters().Length == arity + 1)
            .MakeGenericMethod(typeof(int));
        object?[] arguments = new object?[arity + 1];
        arguments[0] = source;
        method.Invoke(null, arguments);
        return arguments[1..];
    }

    [Theory]
    [MemberData(nameof(PrefixCases))]
    public void EveryArityReadsOnePrefixAndDisposesItsEnumerator(int arity, int length)
    {
        var source = new SinglePassSequence(length);
        var values = Deconstruct(source, arity);
        var expected = Enumerable.Range(0, arity).Select(i => i < length ? i + 11 : 0);
        Assert.Equal(expected, values.Cast<int>());
        Assert.Equal(1, source.Enumerations);
        Assert.Equal(Math.Min(arity, length), source.Reads);
        Assert.True(source.Disposed);
    }

    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    [InlineData(7)]
    [InlineData(8)]
    public void EveryArityDisposesWhenItsLastRequestedElementThrows(int arity)
    {
        var source = new SinglePassSequence(10, arity - 1);
        var error = Assert.Throws<TargetInvocationException>(() => Deconstruct(source, arity));
        Assert.Equal("injected enumeration failure", Assert.IsType<IOException>(error.InnerException).Message);
        Assert.Equal(arity - 1, source.Reads);
        Assert.Equal(1, source.Enumerations);
        Assert.True(source.Disposed);
    }
}
