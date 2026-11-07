using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests
{
    /// <summary>The members of the triplet permissions a protocol request does not reach.</summary>
    public sealed class TripletsPermissionsDirectTests
    {
        private static void WithViews(Action<PermissionsDecoratorTests.World, TripletsOverDoublets<uint>, TripletsPermissionsDecorator<uint>, TripletsPermissionsDecorator<uint>> test)
        {
            PermissionsDecoratorTests.WithWorld(w =>
            {
                var triplets = new TripletsOverDoublets<uint>(w.Links);
                test(w, triplets,
                    new TripletsPermissionsDecorator<uint>(triplets, w.Policy, () => w.V["boss"]),
                    new TripletsPermissionsDecorator<uint>(triplets, w.Policy, () => 0u));
            });
        }

        [Fact]
        public void Markers_And_Usages_Come_From_The_Store_Below()
        {
            WithViews((_, triplets, admin, _) =>
            {
                var point = admin.CreateTripletPoint();
                Assert.Equal(triplets.Markers, admin.Markers);
                Assert.Equal(triplets.DoubletUsages(point), admin.DoubletUsages(point));
            });
        }

        [Fact]
        public void Roles_And_Counts_Show_Only_What_The_Subject_Reads()
        {
            WithViews((w, triplets, admin, guest) =>
            {
                var from = admin.CreateTripletPoint();
                var triplet = admin.CreateTriplet(w.T["Secret"], from, from);
                var everything = new TripletQuery<uint>();

                Assert.Equal(DoubletRole.Triplet, admin.RoleOf(triplet));
                Assert.Null(guest.RoleOf(triplet));
                Assert.Null(admin.RoleOf(uint.MaxValue - 1));
                Assert.Equal(triplets.CountTriplets(everything), admin.CountTriplets(everything));
                Assert.Equal(0, guest.CountTriplets(everything));
            });
        }

        [Fact]
        public void Choosing_The_Address_Of_A_Point_Is_A_Reference()
        {
            WithViews((w, _, admin, guest) =>
            {
                const uint Free = 5000;
                Assert.Throws<ForbiddenException>(() => guest.CreateTripletPointAt(Free));

                admin.CreateTripletPointAt(Free);

                Assert.Equal(new Link<uint>(Free, Free, Free), new Link<uint>(w.Links.GetLink(Free)));
                Assert.Throws<InvalidOperationException>(() => admin.CreateTripletPointAt(Free));
            });
        }
    }
}
