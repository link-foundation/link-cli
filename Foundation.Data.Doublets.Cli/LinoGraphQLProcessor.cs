using Platform.Data.Doublets;
using Platform.Protocols.Lino;
using System.Text;
using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli
{
    public class LinoGraphQLProcessor
    {
        private readonly NamedLinksDecorator<uint> _links;
        private readonly Parser _parser;

        public LinoGraphQLProcessor(NamedLinksDecorator<uint> links)
        {
            _links = links;
            _parser = new Parser();
        }

        public class GraphQLQuery
        {
            public string Query { get; set; } = "";
            public Dictionary<string, object>? Variables { get; set; }
            public string? OperationName { get; set; }
        }

        public class GraphQLResponse
        {
            public string? Data { get; set; }
            public List<GraphQLError>? Errors { get; set; }
        }

        public class GraphQLError
        {
            public string Message { get; set; } = "";
            public List<GraphQLLocation>? Locations { get; set; }
            public string[]? Path { get; set; }
        }

        public class GraphQLLocation
        {
            public int Line { get; set; }
            public int Column { get; set; }
        }

        public GraphQLResponse ProcessLinoGraphQLQuery(string queryString, Dictionary<string, object>? variables = null)
        {
            try
            {
                // Parse the LINO GraphQL query
                var parsedQuery = ParseLinoGraphQLQuery(queryString);
                
                // Execute the query
                var result = ExecuteQuery(parsedQuery, variables);
                
                return new GraphQLResponse
                {
                    Data = result
                };
            }
            catch (Exception ex)
            {
                return new GraphQLResponse
                {
                    Errors = new List<GraphQLError>
                    {
                        new GraphQLError
                        {
                            Message = ex.Message
                        }
                    }
                };
            }
        }

        private LinoGraphQLQueryAst ParseLinoGraphQLQuery(string queryString)
        {
            // Parse LINO notation to extract GraphQL-like structure
            var parsedLinks = _parser.Parse(queryString);
            
            if (parsedLinks.Count == 0)
            {
                throw new ArgumentException("Empty query provided");
            }

            // For simplicity, assume the query structure is:
            // (query (fieldName (selection)))
            var rootLink = parsedLinks[0];
            
            return new LinoGraphQLQueryAst
            {
                Operation = rootLink.Id ?? "query",
                Fields = ExtractFields(rootLink.Values?.ToList() ?? new List<LinoLink>())
            };
        }

        private List<LinoGraphQLField> ExtractFields(List<LinoLink> links)
        {
            var fields = new List<LinoGraphQLField>();
            
            foreach (var link in links)
            {
                var field = new LinoGraphQLField
                {
                    Name = link.Id ?? "unknown",
                    Arguments = new Dictionary<string, object>(),
                    SelectionSet = link.Values?.Any() == true ? ExtractFields(link.Values.ToList()) : null
                };
                fields.Add(field);
            }
            
            return fields;
        }

        private string ExecuteQuery(LinoGraphQLQueryAst query, Dictionary<string, object>? variables)
        {
            var result = new StringBuilder();
            result.Append("(");
            
            foreach (var field in query.Fields)
            {
                var fieldResult = ExecuteField(field, variables);
                if (!string.IsNullOrEmpty(fieldResult))
                {
                    result.Append(fieldResult);
                }
            }
            
            result.Append(")");
            return result.ToString();
        }

        private string ExecuteField(LinoGraphQLField field, Dictionary<string, object>? variables)
        {
            return field.Name switch
            {
                "links" => ExecuteLinksQuery(field),
                "link" => ExecuteLinkQuery(field, variables),
                "schema" => ExecuteSchemaQuery(),
                "__schema" => ExecuteIntrospectionQuery(),
                _ => ExecuteCustomField(field, variables)
            };
        }

        private string ExecuteLinksQuery(LinoGraphQLField field)
        {
            var result = new StringBuilder();
            result.Append($"({field.Name} ");
            
            var any = _links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);
            var links = new List<string>();
            
            _links.Each(query, link =>
            {
                var doubletLink = new DoubletLink(link);
                var formattedLink = FormatLinkForGraphQL(doubletLink, field.SelectionSet);
                links.Add(formattedLink);
                return _links.Constants.Continue;
            });
            
            result.Append(string.Join(" ", links));
            result.Append(")");
            
            return result.ToString();
        }

        private string ExecuteLinkQuery(LinoGraphQLField field, Dictionary<string, object>? variables)
        {
            // Extract link ID from arguments or variables
            uint linkId = 1; // Default
            
            if (field.Arguments?.ContainsKey("id") == true)
            {
                if (uint.TryParse(field.Arguments["id"].ToString(), out var id))
                {
                    linkId = id;
                }
            }
            
            // Check if link exists by trying to query it
            var exists = false;
            _links.Each(new DoubletLink(linkId, _links.Constants.Any, _links.Constants.Any), link =>
            {
                exists = true;
                return _links.Constants.Break;
            });
            
            if (!exists)
            {
                return "";
            }
            
            // Get the actual link using the Each method
            DoubletLink? actualLink = null;
            _links.Each(new DoubletLink(linkId, _links.Constants.Any, _links.Constants.Any), link =>
            {
                actualLink = new DoubletLink(link);
                return _links.Constants.Break;
            });
            
            if (actualLink == null)
            {
                return "";
            }
            
            return $"({field.Name} {FormatLinkForGraphQL(actualLink.Value, field.SelectionSet)})";
        }

        private string ExecuteSchemaQuery()
        {
            return "(schema (types (Link (fields (id source target)))))";
        }

        private string ExecuteIntrospectionQuery()
        {
            return "(__schema (__type (name: \"Link\") (fields (id source target))))";
        }

        private string ExecuteCustomField(LinoGraphQLField field, Dictionary<string, object>? variables)
        {
            // For custom fields, try to match against existing link names or IDs
            if (uint.TryParse(field.Name, out var linkId))
            {
                // Check if link exists by trying to query it
                var linkExists = false;
                _links.Each(new DoubletLink(linkId, _links.Constants.Any, _links.Constants.Any), link =>
                {
                    linkExists = true;
                    return _links.Constants.Break;
                });
                
                if (linkExists)
                {
                    // Get the actual link using the Each method
                    DoubletLink? actualLink = null;
                    _links.Each(new DoubletLink(linkId, _links.Constants.Any, _links.Constants.Any), link =>
                    {
                        actualLink = new DoubletLink(link);
                        return _links.Constants.Break;
                    });
                    
                    if (actualLink != null)
                    {
                        return FormatLinkForGraphQL(actualLink.Value, field.SelectionSet);
                    }
                }
            }
            
            return $"({field.Name})";
        }

        private string FormatLinkForGraphQL(DoubletLink link, List<LinoGraphQLField>? selectionSet)
        {
            if (selectionSet == null || !selectionSet.Any())
            {
                return _links.Format(link);
            }
            
            var result = new StringBuilder();
            result.Append("(");
            
            foreach (var field in selectionSet)
            {
                var value = field.Name switch
                {
                    "id" => link.Index.ToString(),
                    "source" => link.Source.ToString(),
                    "target" => link.Target.ToString(),
                    _ => ""
                };
                
                if (!string.IsNullOrEmpty(value))
                {
                    result.Append($"({field.Name}: {value})");
                }
            }
            
            result.Append(")");
            return result.ToString();
        }
    }

    public class LinoGraphQLQueryAst
    {
        public string Operation { get; set; } = "query";
        public List<LinoGraphQLField> Fields { get; set; } = new();
    }

    public class LinoGraphQLField
    {
        public string Name { get; set; } = "";
        public Dictionary<string, object> Arguments { get; set; } = new();
        public List<LinoGraphQLField>? SelectionSet { get; set; }
    }
}