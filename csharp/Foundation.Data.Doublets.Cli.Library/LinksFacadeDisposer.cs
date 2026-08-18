using System;
using System.Collections.Generic;
using System.Reflection;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// Releases every <see cref="IDisposable"/> links facade reachable from a decorator chain.
    /// </summary>
    /// <remarks>
    /// Links decorators wrap each other and ultimately own memory-mapped file handles. On POSIX a
    /// mapped file can still be unlinked, so leaking those handles goes unnoticed; Windows uses
    /// mandatory locking and fails the delete with <see cref="System.IO.IOException"/>. Disposing the
    /// whole chain keeps behaviour identical on every platform.
    /// </remarks>
    public static class LinksFacadeDisposer
    {
        /// <summary>
        /// Disposes <paramref name="facade"/> and every inner links facade it references, innermost first.
        /// </summary>
        /// <param name="facade">The outermost facade, or <see langword="null"/>.</param>
        public static void Dispose(object? facade)
        {
            var visited = new HashSet<object>(ReferenceEqualityComparer.Instance);
            Dispose(facade, visited);
        }

        private static void Dispose(object? facade, HashSet<object> visited)
        {
            if (facade is null || !visited.Add(facade))
            {
                return;
            }

            foreach (var inner in EnumerateInnerLinks(facade))
            {
                Dispose(inner, visited);
            }

            if (facade is IDisposable disposable)
            {
                disposable.Dispose();
            }
        }

        private static IEnumerable<object?> EnumerateInnerLinks(object facade)
        {
            for (var type = facade.GetType(); type is not null; type = type.BaseType)
            {
                foreach (var field in type.GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly))
                {
                    if (IsLinksFacade(field.FieldType))
                    {
                        yield return field.GetValue(facade);
                    }
                }
            }
        }

        private static bool IsLinksFacade(Type type)
        {
            if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(ILinks<>))
            {
                return true;
            }
            foreach (var @interface in type.GetInterfaces())
            {
                if (@interface.IsGenericType && @interface.GetGenericTypeDefinition() == typeof(ILinks<>))
                {
                    return true;
                }
            }
            return false;
        }
    }
}
