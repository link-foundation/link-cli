using System.Net;
using System.Text;
using Foundation.Data.Doublets.Cli.Graphql;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>The GraphQL executor on shapes the shared vectors leave out, and with a store that fails underneath it.</summary>
public sealed class GraphqlRareTests
{
    private static readonly Auth Admin = new(null, true);
    private static readonly Auth Nobody = new(null, false);
    private static readonly Settings Plain = new(Encoding.UTF8.GetBytes("secret"), null);

    private sealed class Temporary : IDisposable
    {
        private readonly string _file = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());

        public Temporary() => Links = new NamedTypesDecorator<uint>(_file, false);

        public NamedTypesDecorator<uint> Links { get; }

        public void Dispose()
        {
            Links.Dispose();
            foreach (var file in Directory.GetFiles(Path.GetDirectoryName(_file)!, Path.GetFileName(_file) + "*"))
            {
                File.Delete(file);
            }
        }
    }

    /// <summary>Allows so many more writes, then refuses every write, undoing included.</summary>
    private sealed class RunsOut(INamedTypesLinks<uint> links) : LinksDecoratorBase<uint>(links), INamedTypesLinks<uint>
    {
        public int Allowed { get; set; } = int.MaxValue;

        public QueryLimits? QueryLimits => links.QueryLimits;

        private void Spend()
        {
            if (Allowed-- <= 0)
            {
                throw new InvalidOperationException("the store is out of order");
            }
        }

        public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            Spend();
            return base.Create(substitution, handler);
        }

        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            Spend();
            return base.Update(restriction, substitution, handler);
        }

        public override uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler)
        {
            Spend();
            return base.Delete(restriction, handler);
        }

        public string? GetName(uint id) => links.GetName(id);
        public uint SetName(uint id, string name) => links.SetName(id, name);
        public uint GetByName(string name) => links.GetByName(name);
        public void RemoveName(uint id) => links.RemoveName(id);
    }

    private static string Ask(DeepStore store, string text, Auth? auth = null) =>
        Exec.Execute(store, Plain, text, null, Json.Null, auth ?? Admin).Response.Write();

    private static string Insert(long id, string parts = "") =>
        $"mutation{{insert_links_one(object:{{id:{id},type_id:{id}{parts}}}){{id}}}}";

    private static void Inserted(DeepStore store, long id, string parts = "") =>
        Assert.Equal($"{{\"data\":{{\"insert_links_one\":{{\"id\":{id}}}}}}}", Ask(store, Insert(id, parts)));

    [Fact]
    public void KeyedLookupsOfEveryTableUseTheirOwnKey()
    {
        using var file = new Temporary();
        var store = new DeepStore(file.Links, null);
        Assert.Equal(
            "{\"data\":{\"handlers_by_pk\":null,\"can_by_pk\":null}}",
            Ask(store, "{handlers_by_pk(handler_id:1){handler_id} can_by_pk(rule_id:1){rule_id}}"));
    }

    [Fact]
    public void WithoutTheCoreThereAreNoSelectors()
    {
        using var file = new Temporary();
        Assert.Equal("{\"data\":{\"selectors\":[]}}", Ask(new DeepStore(file.Links, null), "{selectors{item_id}}"));
    }

    [Fact]
    public void AnOrderThatStopsAtARelationComparesNothing()
    {
        using var file = new Temporary();
        var store = new DeepStore(file.Links, null);
        Inserted(store, 7);
        Inserted(store, 8, ",from_id:7");
        Assert.Equal(
            "{\"data\":{\"links\":[{\"id\":7},{\"id\":8}]}}",
            Ask(store, "{links(order_by:[{from:{}},{id:asc}]){id}}"));
        Assert.Equal(
            "{\"data\":{\"links\":[{\"id\":7},{\"id\":8}]}}",
            Ask(store, "{links(order_by:{from:asc}){id}}"));
    }

    [Fact]
    public void AConditionThatCannotHoldReadsNoRows()
    {
        using var file = new Temporary();
        var store = new DeepStore(file.Links, null);
        Inserted(store, 7);
        Assert.Equal(
            "{\"data\":{\"links\":[]}}",
            Ask(store, "{links(where:{from_id:{_eq:7},_and:[{from_id:{_eq:8}}]}){id}}"));
    }

    [Fact]
    public void IntrospectionTypesTheEnumValuesItIsAskedFor()
    {
        using var file = new Temporary();
        var answer = Json.Parse(Ask(new DeepStore(file.Links, null), "{__schema{types{kind enumValues{__typename}}}}"));
        var values = answer.Get("data")!.Get("__schema")!.Get("types")!.Items
            .SelectMany(type => type.Get("enumValues") is { IsNull: false } found ? found.Items : [])
            .ToList();
        Assert.NotEmpty(values);
        Assert.All(values, value => Assert.Equal("__EnumValue", value.Get("__typename")!.AsStr()));
    }

    [Fact]
    public void ASyntaxErrorNamesAStringItFound()
    {
        var error = Assert.Throws<FormatException>(() => Document.Parse("{ \"x\" }"));
        Assert.Equal("Expected a name, found Some(String(\"x\")).", error.Message);
    }

    [Fact]
    public void RowsThatAreNotThereAreRefusedByName()
    {
        using var file = new Temporary();
        var store = new DeepStore(file.Links, null);
        Assert.Equal("Link 0 is not a triplet.", Assert.Throws<InvalidOperationException>(() => store.DeleteLink(0, 0)).Message);
        Assert.Equal("No value row 99999.", Assert.Throws<InvalidOperationException>(() => store.UpdateValue(0, 99999, Json.Int(1))).Message);
        Assert.Equal("No value row 99999.", Assert.Throws<InvalidOperationException>(() => store.DeleteValue(0, 99999)).Message);
    }

    [Fact]
    public void AStoredNumberThatIsNotJsonReadsAsNullWhenTheStoreReopens()
    {
        using var file = new Temporary();
        var store = new DeepStore(file.Links, null);
        Inserted(store, 7, ",number:{data:{value:5}}");
        var row = store.Values.First().Key;
        file.Links.SetName(row, $"{row}:not json");

        var reopened = new DeepStore(file.Links, null);

        Assert.Equal("{\"data\":{\"numbers\":[{\"value\":null}]}}", Ask(reopened, "{numbers{value}}"));
    }

    [Fact]
    public void AGuestOfADatabaseWithUsersJoinsThem()
    {
        using var file = new Temporary();
        var store = new DeepStore(file.Links, null);
        store.LoadPackage(Json.Parse(GraphqlTests.Vectors("deep-core.json")));
        Inserted(store, 8001, ",string:{data:{value:\"deep\"}}");
        Inserted(store, 8002);
        Inserted(store, 8003, ",from_id:8001,to_id:8002,string:{data:{value:\"users\"}}");

        var guest = Json.Parse(Ask(store, "{guest{linkId}}", Nobody)).Get("data")!.Get("guest")!.Get("linkId")!.AsI64();

        var joined = Json.Parse(Ask(store, $"{{links(where:{{from_id:{{_eq:8002}},to_id:{{_eq:{guest}}}}}){{id}}}}"));
        Assert.Single(joined.Get("data")!.Get("links")!.Items);
    }

    [Fact]
    public void AGuestThatCannotBeWrittenLeavesNothingBehind()
    {
        using var file = new Temporary();
        var failing = new RunsOut(file.Links);
        var store = new DeepStore(failing, null);
        var before = file.Links.Count(Array.Empty<uint>());
        failing.Allowed = 0;

        var answer = Ask(store, "{guest{linkId}}", Nobody);

        Assert.Contains("the store is out of order", answer, StringComparison.Ordinal);
        Assert.False(store.InMutation);
        Assert.Equal(before, file.Links.Count(Array.Empty<uint>()));
    }

    [Fact]
    public void AMutationThatCannotEvenBeUndoneReportsTheFailure()
    {
        using var file = new Temporary();
        var failing = new RunsOut(file.Links);
        var store = new DeepStore(failing, null);
        failing.Allowed = 1;

        var answer = Ask(store, Insert(7));

        Assert.Contains("the store is out of order", answer, StringComparison.Ordinal);
        Assert.False(store.Admin);
    }

    [Fact]
    public void AServerThatCannotServeSaysSo()
    {
        using var file = new Temporary();
        var store = new DeepStore(file.Links, null);
        var server = new GraphqlServer(new IPEndPoint(IPAddress.Loopback, 0));
        server.Dispose();

        var error = Assert.Throws<InvalidOperationException>(() => server.Serve(store, Plain));

        Assert.IsType<ObjectDisposedException>(error.InnerException);
    }
}
