using Xunit;
using Foundation.Data.Doublets.Cli;
using System.IO;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests
{
  public class StorablePatternManagerTests
  {
    private readonly string _tempFilePath;

    public StorablePatternManagerTests()
    {
      _tempFilePath = Path.GetTempFileName();
    }

    public void Dispose()
    {
      if (File.Exists(_tempFilePath))
      {
        File.Delete(_tempFilePath);
      }
    }

    [Fact]
    public void AddPattern_ShouldStorePatternCorrectly()
    {
      // Arrange
      var manager = new StorablePatternManager(_tempFilePath);
      var query = "((1 1)) ((1 2))";
      
      // Act
      manager.AddPattern(query, "Test pattern");
      
      // Assert
      var patterns = manager.GetActivePatterns();
      Assert.Single(patterns);
      Assert.Equal(query, patterns[0].Query);
      Assert.Equal("Test pattern", patterns[0].Description);
      Assert.True(patterns[0].IsActive);
    }

    [Fact]
    public void RemovePattern_ShouldRemoveExistingPattern()
    {
      // Arrange
      var manager = new StorablePatternManager(_tempFilePath);
      var query = "((1 1)) ((1 2))";
      manager.AddPattern(query);
      
      // Act
      bool removed = manager.RemovePattern(query);
      
      // Assert
      Assert.True(removed);
      var patterns = manager.GetActivePatterns();
      Assert.Empty(patterns);
    }

    [Fact]
    public void RemovePattern_ShouldReturnFalseForNonExistentPattern()
    {
      // Arrange
      var manager = new StorablePatternManager(_tempFilePath);
      var query = "((1 1)) ((1 2))";
      
      // Act
      bool removed = manager.RemovePattern(query);
      
      // Assert
      Assert.False(removed);
    }

    [Fact]
    public void GetActivePatterns_ShouldReturnOnlyActivePatterns()
    {
      // Arrange
      var manager = new StorablePatternManager(_tempFilePath);
      manager.AddPattern("((1 1)) ((1 2))", "Pattern 1");
      manager.AddPattern("((2 2)) ((2 3))", "Pattern 2");
      
      // Act
      var patterns = manager.GetActivePatterns();
      
      // Assert
      Assert.Equal(2, patterns.Count);
      Assert.All(patterns, p => Assert.True(p.IsActive));
    }

    [Fact]
    public void RemoveAllPatterns_ShouldClearAllPatterns()
    {
      // Arrange
      var manager = new StorablePatternManager(_tempFilePath);
      manager.AddPattern("((1 1)) ((1 2))");
      manager.AddPattern("((2 2)) ((2 3))");
      
      // Act
      manager.RemoveAllPatterns();
      
      // Assert
      var patterns = manager.GetActivePatterns();
      Assert.Empty(patterns);
    }

    [Fact]
    public void LoadPatterns_ShouldHandleNonExistentFile()
    {
      // Arrange
      var nonExistentPath = Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString());
      
      // Act & Assert (should not throw)
      var manager = new StorablePatternManager(nonExistentPath);
      var patterns = manager.GetActivePatterns();
      Assert.Empty(patterns);
    }

    [Fact]
    public void LoadPatterns_ShouldHandleCorruptedFile()
    {
      // Arrange
      File.WriteAllText(_tempFilePath, "invalid json content");
      
      // Act & Assert (should not throw)
      var manager = new StorablePatternManager(_tempFilePath);
      var patterns = manager.GetActivePatterns();
      Assert.Empty(patterns);
    }

    [Fact]
    public void PersistenceTest_ShouldMaintainPatternsAcrossInstances()
    {
      // Arrange
      var query1 = "((1 1)) ((1 2))";
      var query2 = "((2 2)) ((2 3))";
      
      // Act - First instance
      var manager1 = new StorablePatternManager(_tempFilePath);
      manager1.AddPattern(query1, "Pattern 1");
      manager1.AddPattern(query2, "Pattern 2");
      
      // Act - Second instance
      var manager2 = new StorablePatternManager(_tempFilePath);
      var patterns = manager2.GetActivePatterns();
      
      // Assert
      Assert.Equal(2, patterns.Count);
      Assert.Contains(patterns, p => p.Query == query1);
      Assert.Contains(patterns, p => p.Query == query2);
    }
  }
}