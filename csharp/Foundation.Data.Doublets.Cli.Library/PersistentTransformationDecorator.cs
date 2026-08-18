using Link.Foundation.Links.Notation;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

using DoubletLink = Platform.Data.Doublets.Link<uint>;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

namespace Foundation.Data.Doublets.Cli;

public enum PersistentTransformationKind
{
    Once,
    Always
}

public sealed record PersistentTransformation(
  uint Root,
  PersistentTransformationKind Kind,
  string Condition,
  string Substitution)
{
    public string Query => $"({Condition} {Substitution})";
}

public sealed class PersistentTransformationDecorator : LinksDecoratorBase<uint>, INamedTypesLinks<uint>
{
    private const string InternalNamePrefix = "__persistent_transformation:";

    private readonly INamedTypesLinks<uint> _namedLinks;
    private readonly INamedTypesLinks<uint> _triggerLinks;
    private readonly bool _trace;
    private bool _applyingTriggers;
    private bool _suppressTriggers;

    public bool AutoCreateMissingReferences { get; set; }

    public PersistentTransformationDecorator(
      INamedTypesLinks<uint> links,
      INamedTypesLinks<uint> triggerLinks,
      bool trace = false)
      : base(links)
    {
        _namedLinks = links;
        _triggerLinks = triggerLinks;
        _trace = trace;
    }

    public static string MakeTriggersDatabaseFilename(string databaseFilename)
    {
        var filenameWithoutExtension = Path.GetFileNameWithoutExtension(databaseFilename);
        var directory = Path.GetDirectoryName(databaseFilename);
        return Path.Combine(directory ?? string.Empty, $"{filenameWithoutExtension}.triggers.links");
    }

    public uint StoreTrigger(PersistentTransformationKind kind, string query)
    {
        var parsed = PersistentTransformationQuery.Parse(query);
        return WithoutTriggerApplication(() =>
        {
            var schema = EnsureSchema();
            var conditionText = EnsureNamedPoint(_triggerLinks, ConditionTextName(parsed.Condition));
            var substitutionText = EnsureNamedPoint(_triggerLinks, SubstitutionTextName(parsed.Substitution));
            var conditionRecord = _triggerLinks.GetOrCreate(schema.Condition, conditionText);
            var substitutionRecord = _triggerLinks.GetOrCreate(schema.Substitution, substitutionText);
            var payload = _triggerLinks.GetOrCreate(conditionRecord, substitutionRecord);
            var triggerType = kind == PersistentTransformationKind.Always ? schema.Always : schema.Once;
            var root = _triggerLinks.GetOrCreate(triggerType, payload);
            Trace($"Stored {kind} trigger #{root}: {parsed.Query}");
            return root;
        });
    }

    public int RemoveTriggers(string query)
    {
        var parsed = PersistentTransformationQuery.Parse(query);
        return WithoutTriggerApplication(() =>
        {
            var matchingTriggers = GetTriggers()
          .Where(trigger => trigger.Condition == parsed.Condition && trigger.Substitution == parsed.Substitution)
          .ToList();

            foreach (var trigger in matchingTriggers)
            {
                DeleteTriggerRoot(trigger.Root);
            }

            return matchingTriggers.Count;
        });
    }

    public IReadOnlyList<PersistentTransformation> GetTriggers()
    {
        if (!TryGetSchema(out var schema))
        {
            return [];
        }

        var linksByIndex = AllLinks(_triggerLinks).ToDictionary(link => link.Index);
        var triggers = new List<PersistentTransformation>();

        foreach (var link in linksByIndex.Values.OrderBy(link => link.Index))
        {
            var kind = link.Source == schema.Always
              ? PersistentTransformationKind.Always
              : link.Source == schema.Once
                ? PersistentTransformationKind.Once
                : (PersistentTransformationKind?)null;

            if (kind is null || !linksByIndex.TryGetValue(link.Target, out var payload))
            {
                continue;
            }

            if (!linksByIndex.TryGetValue(payload.Source, out var conditionRecord)
                || !linksByIndex.TryGetValue(payload.Target, out var substitutionRecord)
                || conditionRecord.Source != schema.Condition
                || substitutionRecord.Source != schema.Substitution)
            {
                continue;
            }

            var condition = DecodeTextName(_triggerLinks.GetName(conditionRecord.Target), "condition");
            var substitution = DecodeTextName(_triggerLinks.GetName(substitutionRecord.Target), "substitution");
            if (condition is null || substitution is null)
            {
                continue;
            }

            triggers.Add(new PersistentTransformation(link.Index, kind.Value, condition, substitution));
        }

        return triggers;
    }

    public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
    {
        return RunWriteOperation(() => _links.Create(substitution, handler));
    }

    public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
    {
        return RunWriteOperation(() => _links.Update(restriction, substitution, handler));
    }

    public override uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler)
    {
        return RunWriteOperation(() => _links.Delete(restriction, handler));
    }

    public override uint Each(IList<uint>? restriction, ReadHandler<uint>? handler)
    {
        return _links.Each(restriction, handler);
    }

    public string? GetName(uint link)
    {
        return _namedLinks.GetName(link);
    }

    public uint SetName(uint link, string name)
    {
        return _namedLinks.SetName(link, name);
    }

    public uint GetByName(string name)
    {
        return _namedLinks.GetByName(name);
    }

    public void RemoveName(uint link)
    {
        _namedLinks.RemoveName(link);
    }

    private uint RunWriteOperation(Func<uint> operation)
    {
        var result = operation();
        ApplyTriggersAfterOperation();
        return result;
    }

    private void ApplyTriggersAfterOperation()
    {
        if (_suppressTriggers || _applyingTriggers)
        {
            return;
        }

        var triggers = GetTriggers();
        if (triggers.Count == 0)
        {
            return;
        }

        _applyingTriggers = true;
        try
        {
            foreach (var trigger in triggers)
            {
                var changes = new List<(DoubletLink Before, DoubletLink After)>();
                QueryProcessor.ProcessQuery(this, new QueryProcessor.Options
                {
                    Query = trigger.Query,
                    Trace = _trace,
                    AutoCreateMissingReferences = AutoCreateMissingReferences,
                    ChangesHandler = (before, after) =>
                    {
                        changes.Add((new DoubletLink(before), new DoubletLink(after)));
                        return Constants.Continue;
                    }
                });

                if (changes.Count > 0 && trigger.Kind == PersistentTransformationKind.Once)
                {
                    DeleteTriggerRoot(trigger.Root);
                }
            }
        }
        finally
        {
            _applyingTriggers = false;
        }
    }

    private T WithoutTriggerApplication<T>(Func<T> action)
    {
        var previousSuppressTriggers = _suppressTriggers;
        _suppressTriggers = true;
        try
        {
            return action();
        }
        finally
        {
            _suppressTriggers = previousSuppressTriggers;
        }
    }

    private TriggerSchema EnsureSchema()
    {
        var type = EnsureNamedPoint(_triggerLinks, "Type");
        var trigger = EnsureNamedPoint(_triggerLinks, "Trigger");
        var once = EnsureNamedPoint(_triggerLinks, "Once");
        var always = EnsureNamedPoint(_triggerLinks, "Always");
        var condition = EnsureNamedPoint(_triggerLinks, "Condition");
        var substitution = EnsureNamedPoint(_triggerLinks, "Substitution");

        _triggerLinks.GetOrCreate(type, trigger);
        _triggerLinks.GetOrCreate(trigger, once);
        _triggerLinks.GetOrCreate(trigger, always);
        _triggerLinks.GetOrCreate(type, condition);
        _triggerLinks.GetOrCreate(type, substitution);

        return new TriggerSchema(type, trigger, once, always, condition, substitution);
    }

    private bool TryGetSchema(out TriggerSchema schema)
    {
        var type = _triggerLinks.GetByName("Type");
        var trigger = _triggerLinks.GetByName("Trigger");
        var once = _triggerLinks.GetByName("Once");
        var always = _triggerLinks.GetByName("Always");
        var condition = _triggerLinks.GetByName("Condition");
        var substitution = _triggerLinks.GetByName("Substitution");
        var @null = _triggerLinks.Constants.Null;

        if (type == @null || trigger == @null || once == @null || always == @null || condition == @null || substitution == @null)
        {
            schema = default;
            return false;
        }

        schema = new TriggerSchema(type, trigger, once, always, condition, substitution);
        return true;
    }

    private void DeleteTriggerRoot(uint root)
    {
        if (!_triggerLinks.Exists(root))
        {
            return;
        }

        var rootLink = new DoubletLink(_triggerLinks.GetLink(root));
        _triggerLinks.Delete(rootLink, null);
        Trace($"Deleted trigger #{root}");
    }

    private static uint EnsureNamedPoint(INamedTypesLinks<uint> links, string name)
    {
        var existing = links.GetByName(name);
        if (existing != links.Constants.Null)
        {
            return existing;
        }

        var id = links.CreateAndUpdate(links.Constants.Null, links.Constants.Null);
        links.SetName(id, name);
        links.Update(
          new DoubletLink(id, links.Constants.Null, links.Constants.Null),
          new DoubletLink(id, id, id),
          null);
        return id;
    }

    private static List<DoubletLink> AllLinks(INamedTypesLinks<uint> links)
    {
        var any = links.Constants.Any;
        return links.All(new DoubletLink(any, any, any)).Select(link => new DoubletLink(link)).ToList();
    }

    private static string ConditionTextName(string condition)
    {
        return $"{InternalNamePrefix}condition:{condition}";
    }

    private static string SubstitutionTextName(string substitution)
    {
        return $"{InternalNamePrefix}substitution:{substitution}";
    }

    private static string? DecodeTextName(string? name, string part)
    {
        var prefix = $"{InternalNamePrefix}{part}:";
        return name is not null && name.StartsWith(prefix, StringComparison.Ordinal)
          ? name[prefix.Length..]
          : null;
    }

    private void Trace(string message)
    {
        if (_trace)
        {
            Console.WriteLine($"[PersistentTransformation] {message}");
        }
    }

    private readonly record struct TriggerSchema(uint Type, uint Trigger, uint Once, uint Always, uint Condition, uint Substitution);

    private sealed record PersistentTransformationQuery(string Condition, string Substitution)
    {
        public string Query => $"({Condition} {Substitution})";

        public static PersistentTransformationQuery Parse(string query)
        {
            var parser = new Parser();
            var parsedLinks = parser.Parse(query);
            if (parsedLinks.Count == 0)
            {
                throw new ArgumentException("Persistent transformation query must contain a condition and a substitution.", nameof(query));
            }

            LinoLink condition;
            LinoLink substitution;
            var outerLink = parsedLinks[0];
            if (outerLink.Values is { Count: >= 2 } outerValues)
            {
                condition = outerValues[0];
                substitution = outerValues[1];
            }
            else if (parsedLinks.Count >= 2)
            {
                condition = parsedLinks[0];
                substitution = parsedLinks[1];
            }
            else
            {
                throw new ArgumentException("Persistent transformation query must contain a condition and a substitution.", nameof(query));
            }

            return new PersistentTransformationQuery(Format(condition), Format(substitution));
        }

        private static string Format(LinoLink link)
        {
            if (link.Values is null || link.Values.Count == 0)
            {
                return string.IsNullOrEmpty(link.Id) ? "()" : EscapeReference(link.Id);
            }

            var values = string.Join(" ", link.Values.Select(Format));
            if (string.IsNullOrEmpty(link.Id))
            {
                return $"({values})";
            }

            return $"({EscapeReference(link.Id)}: {values})";
        }

        private static string EscapeReference(string reference)
        {
            if (string.IsNullOrWhiteSpace(reference))
            {
                return string.Empty;
            }

            var hasSingleQuote = reference.Contains('\'');
            var hasDoubleQuote = reference.Contains('"');
            var needsQuoting = reference.Contains(':')
              || reference.Contains('(')
              || reference.Contains(')')
              || reference.Contains(' ')
              || reference.Contains('\t')
              || reference.Contains('\n')
              || reference.Contains('\r')
              || hasSingleQuote
              || hasDoubleQuote;

            if (hasSingleQuote && hasDoubleQuote)
            {
                return $"'{reference.Replace("'", "\\'")}'";
            }

            if (hasDoubleQuote)
            {
                return $"'{reference}'";
            }

            if (hasSingleQuote)
            {
                return $"\"{reference}\"";
            }

            return needsQuoting ? $"'{reference}'" : reference;
        }
    }
}
