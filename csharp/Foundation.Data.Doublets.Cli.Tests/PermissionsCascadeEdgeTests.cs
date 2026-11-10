using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Delegates;
using Xunit;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests
{
    /// <summary>
    /// What a store below the permissions may do on its own during one write, and
    /// writes at the null link. Every other link a write changes is checked by its
    /// net change, so a store's cascades are refused or allowed like direct writes.
    /// </summary>
    public sealed class PermissionsCascadeEdgeTests
    {
        /// <summary>A store whose next update first makes the writes of <see cref="Also"/>, reporting them.</summary>
        private sealed class CascadingLinks : NamedTypesDecorator<uint>
        {
            public CascadingLinks(ILinks<uint> links, string names) : base(links, names) { }

            public Action<WriteHandler<uint>?>? Also { get; set; }

            public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
            {
                var also = Also;
                Also = null;
                also?.Invoke(handler);
                return base.Update(restriction, substitution, handler);
            }
        }

        private static void WithCascades(Action<PermissionsDecoratorTests.World, CascadingLinks> test)
        {
            var (data, names) = (Path.GetTempFileName(), Path.GetTempFileName());
            try
            {
                using var united = new UnitedMemoryLinks<uint>(data);
                using var links = new CascadingLinks(united.DecorateWithAutomaticUniquenessAndUsagesRepointing(), names);
                test(PermissionsDecoratorTests.MakeWorld(links, links), links);
            }
            finally
            {
                File.Delete(data);
                File.Delete(names);
            }
        }

        private static uint Made(CascadingLinks links, WriteHandler<uint>? handler, uint source, uint target)
        {
            var made = 0u;
            links.Create(null, (before, after) =>
            {
                made = after![links.Constants.IndexPart];
                return handler is null ? links.Constants.Continue : handler(before, after);
            });
            links.Update(new[] { made, 0u, 0u }, new[] { made, source, target }, handler);
            return made;
        }

        [Fact]
        public void A_Cascade_That_Changes_A_Hidden_Link_And_Back_Has_No_Net_Change()
        {
            WithCascades((w, links) =>
            {
                var (t, v) = (w.T, w.V);
                var secret = new DoubletLink(links.GetLink(v["secret"]));
                links.Also = handler =>
                {
                    links.Update(new[] { secret.Index, secret.Source, secret.Target }, new[] { secret.Index, t["Note"], secret.Target }, handler);
                    links.Update(new[] { secret.Index, t["Note"], secret.Target }, new[] { secret.Index, secret.Source, secret.Target }, handler);
                };
                w.As(v["alice"]).Update(v["note"], t["Note"], v["shared"]);
                Assert.Equal(secret, new DoubletLink(links.GetLink(v["secret"])));
                Assert.Equal(v["shared"], new DoubletLink(links.GetLink(v["note"])).Target);
            });
        }

        [Fact]
        public void A_Cascade_May_Create_What_The_Subject_May_Write_And_What_Has_No_Type_Yet()
        {
            WithCascades((w, links) =>
            {
                var (t, v) = (w.T, w.V);
                var made = new List<uint>();
                links.Also = handler =>
                {
                    made.Add(Made(links, handler, t["Note"], v["shared"]));
                    var untyped = 0u;
                    links.Create(null, (before, after) =>
                    {
                        untyped = after![links.Constants.IndexPart];
                        return handler is null ? links.Constants.Continue : handler(before, after);
                    });
                    made.Add(untyped);
                };
                w.As(v["alice"]).Update(v["note"], t["Note"], v["shared"]);
                Assert.All(made, link => Assert.True(links.Exists(link)));
            });
        }

        [Fact]
        public void A_Cascade_That_Creates_And_Deletes_A_Link_Has_No_Net_Change()
        {
            WithCascades((w, links) =>
            {
                var (t, v) = (w.T, w.V);
                var made = 0u;
                links.Also = handler =>
                {
                    made = Made(links, handler, t["Secret"], v["shared"]);
                    links.Delete(new[] { made }, handler);
                };
                w.As(v["alice"]).Update(v["note"], t["Note"], v["shared"]);
                Assert.False(links.Exists(made));
            });
        }

        [Fact]
        public void A_Cascade_That_Creates_What_The_Subject_May_Not_Write_Is_Refused_And_Undone()
        {
            WithCascades((w, links) =>
            {
                var (t, v) = (w.T, w.V);
                var before = w.All(links);
                var made = 0u;
                links.Also = handler => made = Made(links, handler, t["Secret"], v["shared"]);
                Assert.Throws<ForbiddenException>(() => w.As(v["alice"]).Update(v["note"], t["Note"], v["shared"]));
                Assert.NotEqual(0u, made);
                Assert.Equal(before, w.All(links));
            });
        }

        [Fact]
        public void A_Cascade_May_Change_A_Link_The_Subject_Just_Created_While_It_Has_No_Type()
        {
            WithCascades((w, links) =>
            {
                var (t, v) = (w.T, w.V);
                var view = w.As(v["alice"]);
                var own = view.Create();
                links.Also = handler => links.Update(new[] { own, 0u, 0u }, new[] { own, 0u, v["secret"] }, handler);
                view.Update(v["note"], t["Note"], v["shared"]);
                Assert.Equal(new DoubletLink(own, 0, v["secret"]), new DoubletLink(links.GetLink(own)));
            });
        }

        [Fact]
        public void A_Handler_That_Breaks_Stops_Hearing_Of_The_Cascade()
        {
            WithCascades((w, links) =>
            {
                var (t, v) = (w.T, w.V);
                links.Also = handler => Made(links, handler, t["Note"], v["shared"]);
                var heard = 0;
                w.As(v["alice"]).Update(new[] { v["note"], t["Note"], v["note"] }, new[] { v["note"], t["Note"], v["shared"] }, (_, _) =>
                {
                    heard++;
                    return links.Constants.Break;
                });
                Assert.Equal(1, heard);
            });
        }

        [Fact]
        public void Writes_At_The_Null_Link_Are_Not_Found_And_Change_Nothing()
        {
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var view = w.As(w.V["alice"]);
                var before = w.All(w.Links);
                Assert.Contains("not found", Assert.Throws<ArgumentException>(() => view.Update(new uint[] { 0, 0, 0 }, new uint[] { 0, 1, 1 }, null)).Message, StringComparison.OrdinalIgnoreCase);
                Assert.Contains("not found", Assert.Throws<ArgumentException>(() => view.Update(Array.Empty<uint>(), new uint[] { 0, 1, 1 }, null)).Message, StringComparison.OrdinalIgnoreCase);
                Assert.Contains("not found", Assert.Throws<ArgumentException>(() => view.Delete(new uint[] { 0 }, null)).Message, StringComparison.OrdinalIgnoreCase);
                Assert.Contains("not found", Assert.Throws<ArgumentException>(() => view.Delete(Array.Empty<uint>(), null)).Message, StringComparison.OrdinalIgnoreCase);
                Assert.Contains("not found", Assert.Throws<ArgumentException>(() => view.Delete((IList<uint>?)null, null)).Message, StringComparison.OrdinalIgnoreCase);
                Assert.Equal(before, w.All(w.Links));
            });
        }

        [Fact]
        public void The_Policy_Answers_With_The_Stores_It_Was_Made_Over()
        {
            PermissionsDecoratorTests.WithWorld(w =>
            {
                Assert.Same(w.Rules, w.Policy.Rules);
                Assert.Same(w.Types, w.Policy.Types);
            });
        }
    }
}
