using System.Text.Json;

namespace Foundation.Data.Doublets.Cli
{
  public class StorablePatternManager
  {
    private readonly string _patternFilePath;
    private readonly List<StoredPattern> _patterns;

    public StorablePatternManager(string patternFilePath)
    {
      _patternFilePath = patternFilePath;
      _patterns = LoadPatterns();
    }

    public void AddPattern(string query, string? description = null)
    {
      var pattern = new StoredPattern
      {
        Id = Guid.NewGuid().ToString(),
        Query = query,
        Description = description,
        CreatedAt = DateTime.UtcNow,
        IsActive = true
      };

      _patterns.Add(pattern);
      SavePatterns();
    }

    public bool RemovePattern(string query)
    {
      var pattern = _patterns.FirstOrDefault(p => p.Query == query);
      if (pattern != null)
      {
        _patterns.Remove(pattern);
        SavePatterns();
        return true;
      }
      return false;
    }

    public void RemoveAllPatterns()
    {
      _patterns.Clear();
      SavePatterns();
    }

    public IReadOnlyList<StoredPattern> GetActivePatterns()
    {
      return _patterns.Where(p => p.IsActive).ToList().AsReadOnly();
    }

    public void ApplyStoredPatternsOnChange(NamedLinksDecorator<uint> links, Action<string, StoredPattern> onPatternExecution)
    {
      var activePatterns = GetActivePatterns();
      foreach (var pattern in activePatterns)
      {
        try
        {
          var options = new AdvancedMixedQueryProcessor.Options
          {
            Query = pattern.Query,
            Trace = false
          };

          AdvancedMixedQueryProcessor.ProcessQuery(links, options);
          onPatternExecution?.Invoke("Success", pattern);
        }
        catch (Exception ex)
        {
          onPatternExecution?.Invoke($"Error: {ex.Message}", pattern);
        }
      }
    }

    private List<StoredPattern> LoadPatterns()
    {
      if (!File.Exists(_patternFilePath))
      {
        return new List<StoredPattern>();
      }

      try
      {
        var json = File.ReadAllText(_patternFilePath);
        var patterns = JsonSerializer.Deserialize<List<StoredPattern>>(json);
        return patterns ?? new List<StoredPattern>();
      }
      catch (Exception)
      {
        return new List<StoredPattern>();
      }
    }

    private void SavePatterns()
    {
      try
      {
        var json = JsonSerializer.Serialize(_patterns, new JsonSerializerOptions
        {
          WriteIndented = true
        });
        File.WriteAllText(_patternFilePath, json);
      }
      catch (Exception)
      {
        // Log or handle error as needed
      }
    }
  }

  public class StoredPattern
  {
    public string Id { get; set; } = string.Empty;
    public string Query { get; set; } = string.Empty;
    public string? Description { get; set; }
    public DateTime CreatedAt { get; set; }
    public bool IsActive { get; set; } = true;
  }
}