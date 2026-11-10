using System;
using System.Collections.Generic;
using System.Linq;
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
        // Dispose is synchronous. Nested owner cleanup must share this thread's
        // traversal, including facade back-references and shared dependencies.
        [ThreadStatic]
        private static HashSet<object>? _activeTraversal;

        /// <summary>
        /// Disposes <paramref name="facade"/> before its inner links facades,
        /// so borrowers finish using dependencies before those dependencies close.
        /// </summary>
        /// <param name="facade">The outermost facade, or <see langword="null"/>.</param>
        public static void Dispose(object? facade)
        {
            if (_activeTraversal is { } active)
            {
                Dispose(facade, active);
                return;
            }
            var visited = new HashSet<object>(ReferenceEqualityComparer.Instance);
            _activeTraversal = visited;
            try { Dispose(facade, visited); }
            finally { _activeTraversal = null; }
        }

        private static void Dispose(object? facade, HashSet<object> visited)
        {
            if (facade is null || !visited.Add(facade))
            {
                return;
            }

            // Cleanup may detach fields or run its own nested traversal.
            // Capture references while they are still available, and stop on
            // borrower failure so its dependencies remain available for retry.
            var innerLinks = EnumerateInnerLinks(facade).ToArray();
            if (facade is IDisposable disposable)
            {
                disposable.Dispose();
            }
            foreach (var inner in innerLinks)
            {
                Dispose(inner, visited);
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
