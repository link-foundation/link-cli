using Xunit;
using Foundation.Data.Doublets.Cli;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class LinoGraphQLProcessorTests
    {
        [Fact]
        public void ProcessLinoGraphQLQuery_WithSimpleLinksQuery_ReturnsExpectedFormat()
        {
            // Arrange
            var tempDb = Path.GetTempFileName();
            var links = new NamedLinksDecorator<uint>(tempDb, false);
            var processor = new LinoGraphQLProcessor(links);
            
            // Create some test links using Update method
            links.Update(null, new uint[] { 1, 1 }, null);
            links.Update(null, new uint[] { 2, 2 }, null);
            
            // Act
            var result = processor.ProcessLinoGraphQLQuery("(query (links (id source target)))");
            
            // Assert
            Assert.NotNull(result);
            Assert.NotNull(result.Data);
            Assert.True(result.Data.Contains("links"));
            Assert.Null(result.Errors);
            
            // Cleanup
            File.Delete(tempDb);
        }

        [Fact]
        public void ProcessLinoGraphQLQuery_WithSingleLinkQuery_ReturnsExpectedFormat()
        {
            // Arrange
            var tempDb = Path.GetTempFileName();
            var links = new NamedLinksDecorator<uint>(tempDb, false);
            var processor = new LinoGraphQLProcessor(links);
            
            // Create a test link
            var linkId = links.Update(null, new uint[] { 1, 1 }, null);
            
            // Act
            var result = processor.ProcessLinoGraphQLQuery($"(query (link (id: {linkId}) (id source target)))");
            
            // Assert
            Assert.NotNull(result);
            Assert.NotNull(result.Data);
            Assert.True(result.Data.Contains("link"));
            Assert.Null(result.Errors);
            
            // Cleanup
            File.Delete(tempDb);
        }

        [Fact]
        public void ProcessLinoGraphQLQuery_WithSchemaIntrospection_ReturnsSchemaInfo()
        {
            // Arrange
            var tempDb = Path.GetTempFileName();
            var links = new NamedLinksDecorator<uint>(tempDb, false);
            var processor = new LinoGraphQLProcessor(links);
            
            // Act
            var result = processor.ProcessLinoGraphQLQuery("(query (__schema))");
            
            // Assert
            Assert.NotNull(result);
            Assert.NotNull(result.Data);
            Assert.True(result.Data.Contains("__schema"));
            Assert.Null(result.Errors);
            
            // Cleanup
            File.Delete(tempDb);
        }

        [Fact]
        public void ProcessLinoGraphQLQuery_WithInvalidQuery_ReturnsError()
        {
            // Arrange
            var tempDb = Path.GetTempFileName();
            var links = new NamedLinksDecorator<uint>(tempDb, false);
            var processor = new LinoGraphQLProcessor(links);
            
            // Act
            var result = processor.ProcessLinoGraphQLQuery("");
            
            // Assert
            Assert.NotNull(result);
            Assert.NotNull(result.Errors);
            Assert.Single(result.Errors);
            Assert.Contains("Empty query", result.Errors[0].Message);
            
            // Cleanup
            File.Delete(tempDb);
        }

        [Fact]
        public void ProcessLinoGraphQLQuery_WithCustomSchemaQuery_ReturnsSchemaInLino()
        {
            // Arrange
            var tempDb = Path.GetTempFileName();
            var links = new NamedLinksDecorator<uint>(tempDb, false);
            var processor = new LinoGraphQLProcessor(links);
            
            // Act
            var result = processor.ProcessLinoGraphQLQuery("(query (schema))");
            
            // Assert
            Assert.NotNull(result);
            Assert.NotNull(result.Data);
            Assert.True(result.Data.Contains("schema"));
            Assert.True(result.Data.Contains("Link"));
            Assert.True(result.Data.Contains("fields"));
            Assert.Null(result.Errors);
            
            // Cleanup
            File.Delete(tempDb);
        }

        [Fact]
        public void ProcessLinoGraphQLQuery_WithVariables_ProcessesCorrectly()
        {
            // Arrange
            var tempDb = Path.GetTempFileName();
            var links = new NamedLinksDecorator<uint>(tempDb, false);
            var processor = new LinoGraphQLProcessor(links);
            
            var linkId = links.Update(null, new uint[] { 1, 1 }, null);
            var variables = new Dictionary<string, object> { { "linkId", linkId } };
            
            // Act
            var result = processor.ProcessLinoGraphQLQuery("(query (link (id source target)))", variables);
            
            // Assert
            Assert.NotNull(result);
            Assert.NotNull(result.Data);
            Assert.Null(result.Errors);
            
            // Cleanup
            File.Delete(tempDb);
        }
    }
}