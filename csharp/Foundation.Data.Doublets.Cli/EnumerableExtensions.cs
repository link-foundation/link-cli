using System;
using System.Collections.Generic;

public static class EnumerableExtensions
{
    public static void Deconstruct<T>(this IEnumerable<T> source, out T first)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
    }

    public static void Deconstruct<T>(this IEnumerable<T> source, out T first, out T second)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
        second = enumerator.MoveNext() ? enumerator.Current : default!;
    }

    public static void Deconstruct<T>(this IEnumerable<T> source, out T first, out T second, out T third)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
        second = enumerator.MoveNext() ? enumerator.Current : default!;
        third = enumerator.MoveNext() ? enumerator.Current : default!;
    }

    public static void Deconstruct<T>(this IEnumerable<T> source, out T first, out T second, out T third, out T fourth)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
        second = enumerator.MoveNext() ? enumerator.Current : default!;
        third = enumerator.MoveNext() ? enumerator.Current : default!;
        fourth = enumerator.MoveNext() ? enumerator.Current : default!;
    }

    public static void Deconstruct<T>(this IEnumerable<T> source, out T first, out T second, out T third, out T fourth, out T fifth)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
        second = enumerator.MoveNext() ? enumerator.Current : default!;
        third = enumerator.MoveNext() ? enumerator.Current : default!;
        fourth = enumerator.MoveNext() ? enumerator.Current : default!;
        fifth = enumerator.MoveNext() ? enumerator.Current : default!;
    }

    public static void Deconstruct<T>(this IEnumerable<T> source, out T first, out T second, out T third, out T fourth, out T fifth, out T sixth)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
        second = enumerator.MoveNext() ? enumerator.Current : default!;
        third = enumerator.MoveNext() ? enumerator.Current : default!;
        fourth = enumerator.MoveNext() ? enumerator.Current : default!;
        fifth = enumerator.MoveNext() ? enumerator.Current : default!;
        sixth = enumerator.MoveNext() ? enumerator.Current : default!;
    }

    public static void Deconstruct<T>(this IEnumerable<T> source, out T first, out T second, out T third, out T fourth, out T fifth, out T sixth, out T seventh)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
        second = enumerator.MoveNext() ? enumerator.Current : default!;
        third = enumerator.MoveNext() ? enumerator.Current : default!;
        fourth = enumerator.MoveNext() ? enumerator.Current : default!;
        fifth = enumerator.MoveNext() ? enumerator.Current : default!;
        sixth = enumerator.MoveNext() ? enumerator.Current : default!;
        seventh = enumerator.MoveNext() ? enumerator.Current : default!;
    }

    public static void Deconstruct<T>(this IEnumerable<T> source, out T first, out T second, out T third, out T fourth, out T fifth, out T sixth, out T seventh, out T eighth)
    {
        using var enumerator = source.GetEnumerator();
        first = enumerator.MoveNext() ? enumerator.Current : default!;
        second = enumerator.MoveNext() ? enumerator.Current : default!;
        third = enumerator.MoveNext() ? enumerator.Current : default!;
        fourth = enumerator.MoveNext() ? enumerator.Current : default!;
        fifth = enumerator.MoveNext() ? enumerator.Current : default!;
        sixth = enumerator.MoveNext() ? enumerator.Current : default!;
        seventh = enumerator.MoveNext() ? enumerator.Current : default!;
        eighth = enumerator.MoveNext() ? enumerator.Current : default!;
    }
}