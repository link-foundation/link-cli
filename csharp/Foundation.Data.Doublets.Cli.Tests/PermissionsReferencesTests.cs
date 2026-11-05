using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Foundation.Data.Doublets.Cli.Protocol;
using Platform.Data;
using Platform.Data.Doublets;
using Xunit;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using World = Foundation.Data.Doublets.Cli.Tests.PermissionsDecoratorTests.World;

namespace Foundation.Data.Doublets.Cli.Tests
{
    /// <summary>
    /// A reference needs a right (issue #161): without it an unreadable address answers exactly as a
    /// missing one. The Rust twin is <c>rust/tests/permissions_references_tests.rs</c>, case for case.
    /// </summary>
    public sealed class PermissionsReferencesTests
    {
        private const uint Missing = 9999u;

        private static string? ErrorOf(Action action)
        {
            try
            {
                action();
                return null;
            }
            catch (ForbiddenException)
            {
                return "forbidden";
            }
            catch (ArgumentException error) when (error.Message.StartsWith("Link not found", StringComparison.Ordinal))
            {
                return "not-found";
            }
        }

        private static string S(uint value) => value.ToString(CultureInfo.InvariantCulture);

        /// <summary>A new link through <paramref name="view"/>, which is a point until it is written.</summary>
        private static uint Make(PermissionsDecorator<uint> view, uint source, uint target)
        {
            var id = view.Create();
            view.Update(id, source, target);
            return id;
        }

        /// <summary>A note of alice's own, as the requests that follow may update it.</summary>
        private static uint OwnNote(World w) => Make(w.As(w.V["alice"]), w.T["Note"], 0u);

        /// <summary>The reply of <paramref name="request"/> as <paramref name="subject"/>.</summary>
        private static string Ask(World w, uint subject, string request)
        {
            var sessions = new LinksSessions(w.Links, new LinksServerOptions { Permissions = w.Policy });
            using var session = sessions.Open(subject);
            return session.Request(request);
        }

        private static void AnswersAlike(Func<uint, uint, uint, string> request, bool notFound) =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var own = OwnNote(w);
                var (note, secret, alice) = (w.T["Note"], w.V["secret"], w.V["alice"]);
                var before = w.All(w.Links);
                var hidden = Ask(w, alice, request(note, secret, own));
                var missing = Ask(w, alice, request(note, Missing, own));
                Assert.StartsWith("(error: ", hidden, StringComparison.Ordinal);
                Assert.Equal(hidden.Replace(S(secret), "N", StringComparison.Ordinal), missing.Replace(S(Missing), "N", StringComparison.Ordinal));
                if (notFound)
                {
                    Assert.Equal($"(error: 'Link not found: {secret}')", hidden);
                }
                Assert.Equal(before, w.All(w.Links));
            });

        [Fact]
        public void ACreateAnswersAlikeForAnUnreadableAndAMissingAddress()
        {
            AnswersAlike((note, a, _) => $"(create: ({note} {a}))", true);
            AnswersAlike((_, a, _) => $"(create: (0 {a}))", true);
        }

        [Fact]
        public void AnUpdateAnswersAlikeForAnUnreadableAndAMissingAddress() =>
            AnswersAlike((note, a, own) => $"(update: ({own} {note} {a}))", true);

        [Fact]
        public void AQueryAnswersAlikeForAnUnreadableAndAMissingAddress()
        {
            AnswersAlike((note, a, _) => $"() (({note} {a}))", false);
            AnswersAlike((_, a, _) => $"(({a}: * *)) ()", false);
        }

        [Fact]
        public void PickingAnAddressAnswersAlikeForAnUnreadableAndAMissingOne() =>
            AnswersAlike((note, a, _) => $"() (({a}: {note} {note}))", false);

        [Fact]
        public void ATypeItMayNotWriteIsForbiddenForHiddenAndMissingAlike() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var own = OwnNote(w);
                foreach (var type in new[] { w.T["Secret"], Missing })
                {
                    Assert.Equal($"(error: 'Forbidden: update {own}')", Ask(w, w.V["alice"], $"(update: ({own} {type} {own}))"));
                }
            });

        [Fact]
        public void TheNameOfAnUnreadableLinkIsAnUnknownName() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (note, secret, alice) = (w.T["Note"], w.V["secret"], w.V["alice"]);
                Assert.Equal(0u, w.As(alice).GetByName("secret"));
                Assert.Null(w.As(alice).GetName(secret));
                foreach (var request in new[] { $"() (({note} NAME))", "((NAME: * *)) ()" })
                {
                    var hidden = Ask(w, alice, request.Replace("NAME", "secret", StringComparison.Ordinal));
                    var missing = Ask(w, alice, request.Replace("NAME", "nothing", StringComparison.Ordinal));
                    Assert.StartsWith("(error: ", hidden, StringComparison.Ordinal);
                    Assert.Equal(hidden.Replace("secret", "N", StringComparison.Ordinal), missing.Replace("nothing", "N", StringComparison.Ordinal));
                }
            });

        [Fact]
        public void TheNameOfAnUnreadableLinkCannotBeTaken() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (note, secret, alice) = (w.V["note"], w.V["secret"], w.V["alice"]);
                Assert.StartsWith("(error: 'Forbidden: ", Ask(w, alice, $"(set-name: ({note} 'secret'))"), StringComparison.Ordinal);
                Assert.StartsWith("(error: 'Forbidden: ", Ask(w, alice, $"() ((secret: {note} {note}))"), StringComparison.Ordinal);
                Assert.Equal(secret, w.Links.GetByName("secret"));
                Assert.Equal("note", w.Links.GetName(note));
            });

        [Fact]
        public void TheViewRefusesItAlone() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (note, secret) = (w.T["Note"], w.V["secret"]);
                var view = w.As(w.V["alice"]);
                var id = view.Create();
                foreach (var address in new[] { secret, Missing })
                {
                    Assert.Equal("not-found", ErrorOf(() => view.Update(id, note, address)));
                    Assert.Equal("not-found", ErrorOf(() => view.Update(id, 0u, address)));
                    Assert.Equal("not-found", ErrorOf(() => LinksExtensions.EnsureCreated(view, address)));
                    Assert.False(view.ReferenceExists(address));
                }
                Assert.Equal(new DoubletLink(id, 0u, 0u), new DoubletLink(w.Links.GetLink(id)));
            });

        /// <summary>A guest creates nothing, so it chooses no address, hidden or missing.</summary>
        [Fact]
        public void AGuestChoosesNoAddress() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                foreach (var address in new[] { w.V["secret"], Missing })
                {
                    Assert.Equal("forbidden", ErrorOf(() => LinksExtensions.EnsureCreated(w.As(0u), address)));
                }
            });

        [Fact]
        public void ARuleCoversItsObjectAndTheLinksOfItsType() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                PermissionPolicy<uint>.Allow(w.Rules, w.Markers, PermissionAccess.Reference, t["User"], t["Secret"]);
                var view = w.As(v["alice"]);
                Make(view, t["Note"], v["secret"]);
                Make(view, t["Note"], t["Secret"]);
                // Not its sibling, and a missing address stays missing.
                var id = view.Create();
                Assert.Equal("not-found", ErrorOf(() => view.Update(id, t["Note"], t["Admin"])));
                Assert.Equal("not-found", ErrorOf(() => view.Update(id, t["Note"], Missing)));
                // With the right the query answers that the address is taken.
                Assert.True(view.ReferenceExists(v["secret"]));
                Assert.False(view.ReferenceExists(Missing));
                Assert.Equal(v["secret"], view.GetByName("secret"));
                var taken = Assert.Throws<InvalidOperationException>(() => LinksExtensions.EnsureCreated(view, v["secret"]));
                Assert.Equal($"Link {v["secret"]} exists", taken.Message);
            });

        [Fact]
        public void TheRightDoesNotAllowReading() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                PermissionPolicy<uint>.Allow(w.Rules, w.Markers, PermissionAccess.Reference, t["User"], t["Secret"]);
                var (alice, secret) = (v["alice"], v["secret"]);
                var view = w.As(alice);
                Assert.True(view.CanReference(secret));
                Assert.False(view.CanRead(new DoubletLink(secret, t["Secret"], 0u)));
                Assert.False(view.Exists(secret));
                Assert.DoesNotContain(w.All(view), link => link.Index == secret);
                Assert.Equal("not-found", ErrorOf(() => view.Delete(secret)));
                Assert.Equal("", Ask(w, alice, $"(each: ({secret}))"));
                Assert.Equal("", Ask(w, alice, "(get-by-name: 'secret')"));
            });

        [Fact]
        public void ARuleAppliesToItsSubjectsOnly() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                PermissionPolicy<uint>.Allow(w.Rules, w.Markers, PermissionAccess.Reference, t["Admin"], t["Secret"]);
                // boss is an Admin, alice only a User, and a guest nobody.
                Assert.True(w.As(v["boss"]).CanReference(v["secret"]));
                Assert.False(w.As(v["alice"]).CanReference(v["secret"]));
                Assert.False(w.As(0u).CanReference(v["secret"]));
            });

        [Fact]
        public void TheObjectOfARuleOfTheSubjectNeedsNoMoreRules() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var view = w.As(v["alice"]);
                // User writes Note: enough to type a link as Note, yet Note stays unread.
                Assert.False(view.Exists(t["Note"]));
                Make(view, t["Note"], 0u);
                // User reads Public, readable links, and the subject itself.
                Make(view, t["Note"], t["Public"]);
                Make(view, t["Note"], v["shared"]);
                Make(view, t["Note"], v["alice"]);
                // Who it says nothing of: the guest.
                Assert.False(w.As(0u).CanReference(t["Note"]));
            });

        [Fact]
        public void AReferenceItHoldsMayStayButCannotBePutBack() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var held = Make(w.As(v["boss"]), t["Note"], v["secret"]);
                var view = w.As(v["alice"]);
                // It stays, while the update changes what the subject may change.
                view.Update(held, t["Shared"], v["secret"]);
                view.Update(held, t["Note"], v["secret"]);
                // Once replaced, it cannot be put back.
                view.Update(held, t["Note"], 0u);
                Assert.Equal("not-found", ErrorOf(() => view.Update(held, t["Note"], v["secret"])));
            });

        [Fact]
        public void AnythingToAnythingRestoresThePreviousBehaviourAsAChoice() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                PermissionPolicy<uint>.Allow(w.Rules, w.Markers, PermissionAccess.Reference, w.Markers.Anything, w.Markers.Anything);
                var (alice, secret) = (v["alice"], v["secret"]);
                var view = w.As(alice);
                Make(view, t["Note"], secret);
                Assert.Equal(secret, view.GetByName("secret"));
                // The address of a hidden link is taken, a free one is not.
                var taken = Assert.Throws<InvalidOperationException>(() => LinksExtensions.EnsureCreated(view, secret));
                Assert.Equal($"Link {secret} exists", taken.Message);
                // A query tells them apart: the link exists, the other one does not.
                Assert.True(view.ReferenceExists(secret));
                Assert.False(view.ReferenceExists(Missing));
                LinksExtensions.EnsureCreated(view, Missing);
                Assert.True(view.Exists(Missing));
                // It reads no more than it did.
                Assert.False(view.Exists(secret));
                // A query refers to it by name, here to the link made above.
                var reply = Ask(w, alice, $"() (({t["Note"]} secret))");
                Assert.False(reply.StartsWith("(error", StringComparison.Ordinal), reply);
            });

        [Fact]
        public void AnAdministratorIsNotAffected() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var boss = v["boss"];
                Assert.StartsWith("() ((", Ask(w, boss, $"() (({t["Note"]} {v["secret"]}))"), StringComparison.Ordinal);
                Assert.StartsWith("() ((", Ask(w, boss, $"() ((12345: {t["Note"]} {t["Note"]}))"), StringComparison.Ordinal);
            });

        /// <summary>Everyone reads what starts at a type it names, and says nothing of references.</summary>
        private sealed class ReadsPublicOnly(uint type, PermissionMarkers<uint> markers) : IPermissionPolicy<uint>
        {
            public PermissionMarkers<uint> Markers => markers;

            public bool CanRead(uint subject, IList<uint> link) => link[1] == type;

            public bool CanWrite(uint subject, IList<uint> link) => true;

            public bool CanCreate(uint subject) => true;
        }

        [Fact]
        public void APolicyWithoutTheRightAllowsReferencesToReadableLinksOnly() =>
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var (t, v) = (w.T, w.V);
                var readable = w.Links.GetOrCreate(t["Public"], 0u);
                var view = new PermissionsDecorator<uint>(w.Links, new ReadsPublicOnly(t["Public"], w.Markers), v["alice"]);
                var id = view.Create();
                view.Update(id, 0u, readable);
                Assert.Equal("not-found", ErrorOf(() => view.Update(id, 0u, v["secret"])));
            });
    }
}
