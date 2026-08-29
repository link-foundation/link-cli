using System.Reflection;

using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    /// <summary>
    /// Every decorator the library ships is a seam: an application that needs
    /// behaviour of its own subclasses one instead of forking the stack. These
    /// tests take that seam for a ride, so re-sealing a class or dropping a
    /// <c>virtual</c> breaks the build rather than silently narrowing the API.
    /// </summary>
    public class ExtensibilityTests
    {
        [Fact]
        public void NamedTypesDecoratorCanBeSubclassed()
        {
            RunWithFiles(dataFile =>
            {
                CountingNamedTypes subclass;
                using (subclass = new CountingNamedTypes(dataFile))
                {
                    var link = subclass.CreateAndUpdate(subclass.Constants.Null, subclass.Constants.Null);
                    subclass.SetName(link, "answer");

                    Assert.Equal("answer", subclass.GetName(link));
                    Assert.Equal(link, subclass.GetByName("answer"));
                }

                Assert.Equal(1, subclass.NamesSet);
                Assert.Equal(1, subclass.NamesRead);
                Assert.Equal(1, subclass.Lookups);
                Assert.True(subclass.Disposed, "Dispose() must reach the derived Dispose(bool) override.");
            });
        }

        [Fact]
        public void TransactionsDecoratorCanBeSubclassed()
        {
            RunWithFiles((dataFile, logFile) =>
            {
                using var dataLinks = new NamedTypesDecorator<uint>(dataFile);
                using var logLinks = new NamedTypesDecorator<uint>(logFile);
                using var subclass = new CountingTransactions(dataLinks, logLinks);

                using (var transaction = subclass.BeginTransaction())
                {
                    subclass.CreateAndUpdate(subclass.Constants.Null, subclass.Constants.Null);
                    transaction.Commit();
                }

                Assert.Equal(1, subclass.TransactionsBegun);
            });
        }

        [Fact]
        public void PersistentTransformationDecoratorCanBeSubclassed()
        {
            RunWithFiles((dataFile, triggerFile) =>
            {
                using var dataLinks = new NamedTypesDecorator<uint>(dataFile);
                using var triggerLinks = new NamedTypesDecorator<uint>(triggerFile);
                var subclass = new CountingTransformations(dataLinks, triggerLinks);

                subclass.StoreTrigger(PersistentTransformationKind.Always, "(((1: 1 1)) ((1: 1 2)))");

                Assert.Equal(1, subclass.TriggersStored);
                Assert.Single(subclass.GetTriggers());
            });
        }

        [Fact]
        public void VersionControlDecoratorCanBeSubclassed()
        {
            RunWithFiles((dataFile, logFile) =>
            {
                var branchesFile = Path.GetTempFileName();
                try
                {
                    using var dataLinks = new NamedTypesDecorator<uint>(dataFile);
                    using var logLinks = new NamedTypesDecorator<uint>(logFile);
                    using var branchLinks = new NamedTypesDecorator<uint>(branchesFile);
                    using var transactions = new TransactionsDecorator(dataLinks, logLinks);
                    using var subclass = new CountingVersionControl(transactions, branchLinks);

                    subclass.Branch("feature");

                    Assert.Equal(1, subclass.BranchesMade);
                    Assert.Contains(subclass.ListBranches(), branch => branch.Name == "feature");
                }
                finally
                {
                    Cleanup(branchesFile);
                    Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(branchesFile));
                }
            });
        }

        /// <summary>
        /// The composition seams stay open even where no test above happens to
        /// subclass them, so a custom front end can replace any layer.
        /// </summary>
        [Theory]
        [InlineData(typeof(NamedTypesDecorator<uint>))]
        [InlineData(typeof(NamedLinksDecorator<uint>))]
        [InlineData(typeof(SimpleLinksDecorator<uint>))]
        [InlineData(typeof(PinnedTypesDecorator<uint>))]
        [InlineData(typeof(TransactionsDecorator))]
        [InlineData(typeof(TransactionsDecorator<uint>))]
        [InlineData(typeof(VersionControlDecorator))]
        [InlineData(typeof(PersistentTransformationDecorator))]
        [InlineData(typeof(NamedLinks<uint>))]
        [InlineData(typeof(PinnedTypes<uint>))]
        [InlineData(typeof(UnicodeStringStorage<uint>))]
        public void PublicTypesStayOpenForExtension(Type type)
        {
            Assert.True(type.IsPublic, $"{type.Name} must be public.");
            Assert.False(type.IsSealed, $"{type.Name} must stay unsealed so it can be subclassed.");

            var library = typeof(NamedTypesDecorator<uint>).Assembly;
            var overridable = type
              .GetMethods(BindingFlags.Public | BindingFlags.Instance)
              .Where(method => method.IsVirtual && !method.IsFinal)
              .Where(method => method.DeclaringType?.Assembly == library);
            Assert.NotEmpty(overridable);
        }

        /// <summary>
        /// Disposable seams follow the <c>Dispose(bool)</c> pattern, which is
        /// what lets a subclass release resources of its own.
        /// </summary>
        [Theory]
        [InlineData(typeof(NamedTypesDecorator<uint>))]
        [InlineData(typeof(NamedLinksDecorator<uint>))]
        [InlineData(typeof(SimpleLinksDecorator<uint>))]
        [InlineData(typeof(TransactionsDecorator<uint>))]
        [InlineData(typeof(VersionControlDecorator))]
        public void DisposableTypesExposeTheProtectedDisposePattern(Type type)
        {
            var dispose = type.GetMethod(
              "Dispose",
              BindingFlags.NonPublic | BindingFlags.Instance,
              [typeof(bool)]);

            Assert.NotNull(dispose);
            Assert.True(dispose!.IsFamily, "Dispose(bool) must be protected.");
            Assert.True(dispose.IsVirtual && !dispose.IsFinal, "Dispose(bool) must be overridable.");
        }

        private sealed class CountingNamedTypes : NamedTypesDecorator<uint>
        {
            public CountingNamedTypes(string databaseFilename) : base(databaseFilename)
            {
            }

            public int NamesSet { get; private set; }
            public int NamesRead { get; private set; }
            public int Lookups { get; private set; }
            public bool Disposed { get; private set; }

            public override uint SetName(uint link, string name)
            {
                NamesSet++;
                return base.SetName(link, name);
            }

            public override string? GetName(uint link)
            {
                NamesRead++;
                return base.GetName(link);
            }

            public override uint GetByName(string name)
            {
                Lookups++;
                return base.GetByName(name);
            }

            protected override void Dispose(bool disposing)
            {
                Disposed = true;
                base.Dispose(disposing);
            }
        }

        private sealed class CountingTransactions : TransactionsDecorator
        {
            public CountingTransactions(INamedTypesLinks<uint> inner, INamedTypesLinks<uint> logStore)
              : base(inner, logStore)
            {
            }

            public int TransactionsBegun { get; private set; }

            public override ITransaction<uint> BeginTransaction()
            {
                TransactionsBegun++;
                return base.BeginTransaction();
            }
        }

        private sealed class CountingTransformations : PersistentTransformationDecorator
        {
            public CountingTransformations(INamedTypesLinks<uint> links, INamedTypesLinks<uint> triggerLinks)
              : base(links, triggerLinks)
            {
            }

            public int TriggersStored { get; private set; }

            public override uint StoreTrigger(PersistentTransformationKind kind, string query)
            {
                TriggersStored++;
                return base.StoreTrigger(kind, query);
            }
        }

        private sealed class CountingVersionControl : VersionControlDecorator
        {
            public CountingVersionControl(TransactionsDecorator transactions, INamedTypesLinks<uint> branchesStore)
              : base(transactions, branchesStore)
            {
            }

            public int BranchesMade { get; private set; }

            public override void Branch(string name, long? from = null)
            {
                BranchesMade++;
                base.Branch(name, from);
            }
        }

        private static void RunWithFiles(Action<string> action)
        {
            var dataFile = Path.GetTempFileName();
            try
            {
                action(dataFile);
            }
            finally
            {
                Cleanup(dataFile);
                Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dataFile));
            }
        }

        private static void RunWithFiles(Action<string, string> action)
        {
            var secondFile = Path.GetTempFileName();
            try
            {
                RunWithFiles(dataFile => action(dataFile, secondFile));
            }
            finally
            {
                Cleanup(secondFile);
                Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(secondFile));
            }
        }

        private static void Cleanup(string path)
        {
            if (File.Exists(path)) File.Delete(path);
        }
    }
}
