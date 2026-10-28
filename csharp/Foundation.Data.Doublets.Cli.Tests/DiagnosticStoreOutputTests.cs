using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Data.Doublets;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class DiagnosticOutputTests
{
    public static IEnumerable<object[]> NamedStoreStages =>
        from typed in new[] { false, true }
        from stage in new[] { "construct", "get-name", "set-name", "get-by-name", "remove-name", "update", "delete" }
        select new object[] { typed, stage };

    [Theory]
    [MemberData(nameof(NamedStoreStages))]
    public void DiagnosticFailurePreservesNamedStoreOperations(bool typed, string stage)
    {
        var directory = Path.Combine(Path.GetTempPath(), "diagnostic-store-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var path = Path.Combine(directory, "data");
            IDisposable Construct() => typed
                ? new NamedTypesDecorator<uint>(path, tracingEnabled: true)
                : new NamedLinksDecorator<uint>(path, tracingEnabled: true);
            using var owned = stage == "construct" ? WithoutConsole(Construct) : Construct();
            var links = Assert.IsAssignableFrom<INamedTypesLinks<uint>>(owned);
            var id = links.CreateAndUpdate(0u, 0u);
            if (stage != "set-name") links.SetName(id, "profile");
            WithoutConsole(() =>
            {
                switch (stage)
                {
                    case "get-name": Assert.Equal("profile", links.GetName(id)); break;
                    case "set-name": links.SetName(id, "profile"); break;
                    case "get-by-name": Assert.Equal(id, links.GetByName("profile")); break;
                    case "remove-name": links.RemoveName(id); break;
                    case "update": links.Update(id, id, 0u); break;
                    case "delete": links.Delete(id); break;
                }
            });
            Assert.Equal(stage != "delete", links.Exists(id));
            Assert.Equal(stage is "remove-name" or "delete" ? null : "profile", links.GetName(id));
            if (stage == "update") Assert.Equal(id, links.GetSource(id));
            Assert.NotEqual(0u, links.CreateAndUpdate(0u, 0u));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void DiagnosticFailurePreservesSimpleStoreConstruction()
    {
        var directory = Path.Combine(Path.GetTempPath(), "diagnostic-simple-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var links = WithoutConsole(() => new SimpleLinksDecorator<uint>(Path.Combine(directory, "data"), tracingEnabled: true));
            var id = links.CreateAndUpdate(0u, 0u);
            Assert.True(links.Exists(id));
            links.Delete(id); Assert.False(links.Exists(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData("create")]
    [InlineData("update")]
    [InlineData("delete")]
    public void DiagnosticFailurePreservesQueryMutations(string operation)
    {
        WithStores((data, _, _) =>
        {
            if (operation != "create") AdvancedMixedQueryProcessor.ProcessQuery(data,
                new AdvancedMixedQueryProcessor.Options { Query = "(() ((1: 1 1)))" });
            var query = operation switch
            {
                "create" => "(() ((1: 1 1)))",
                "update" => "(((1: 1 1)) ((1: 1 2)))",
                _ => "(((1: 1 1)) ())"
            };
            WithoutConsole(() => AdvancedMixedQueryProcessor.ProcessQuery(data,
                new AdvancedMixedQueryProcessor.Options { Query = query, Trace = true, AutoCreateMissingReferences = true }));
            Assert.Equal(operation != "delete", data.Exists(1u));
            if (operation != "delete") Assert.Equal(operation == "update" ? 2u : 1u, data.GetTarget(1u));
            Assert.NotEqual(0u, data.CreateAndUpdate(0u, 0u));
        });
    }

    private sealed class FailingErrorWriter(int allowedWrites) : TextWriter
    {
        public override Encoding Encoding => Encoding.UTF8;
        public override void WriteLine(string? value)
        { if (allowedWrites-- <= 0) throw new IOException("injected server diagnostic failure"); }
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    public void DiagnosticFailurePreservesServerRequestAndReply(int allowedWrites)
    {
        WithStores((data, _, _) =>
        {
            using var server = LinksServer.Bind("127.0.0.1", 0, new LinksServerOptions { Trace = true });
            using var unavailable = new FailingErrorWriter(allowedWrites);
            var previous = Console.Error;
            try
            {
                Console.SetError(unavailable);
                var reply = server.Execute(data, LinoFormat.ParseDocument("() ((1 1))"));
                Assert.Equal("() ((1: 1 1))", LinoFormat.FormatDocument(reply));
                Assert.Equal("(1: 1 1)", LinoFormat.FormatDocument(server.Execute(data, LinoFormat.ParseDocument(""))));
            }
            finally { Console.SetError(previous); }
            Assert.True(data.Exists(1u));
        });
    }
}
