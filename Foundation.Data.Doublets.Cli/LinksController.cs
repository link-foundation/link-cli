using Microsoft.AspNetCore.Mvc;
using Microsoft.Extensions.Configuration;
using Platform.Data.Doublets;
using System.Text;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

namespace Foundation.Data.Doublets.Cli
{
    [ApiController]
    [Route("api/[controller]")]
    public class LinksController : ControllerBase
    {
        private readonly string _dbPath;

        public LinksController(IConfiguration configuration)
        {
            _dbPath = configuration.GetValue<string>("Database:Path") ?? "db.links";
        }

        [HttpGet]
        public async Task<IActionResult> GetAllLinks([FromQuery] bool trace = false)
        {
            try
            {
                var decoratedLinks = new NamedLinksDecorator<uint>(_dbPath, trace);
                var query = "((($i: $s $t)) (($i: $s $t)))";
                var result = await ProcessLinoQueryAsync(decoratedLinks, query, trace);
                return Ok(result);
            }
            catch (Exception ex)
            {
                return BadRequest($"Error: {ex.Message}");
            }
        }

        [HttpPost]
        public async Task<IActionResult> CreateLinks([FromBody] LinoRequest request)
        {
            if (string.IsNullOrWhiteSpace(request.Query))
            {
                return BadRequest("Query is required");
            }

            try
            {
                var decoratedLinks = new NamedLinksDecorator<uint>(_dbPath, request.Trace);
                var result = await ProcessLinoQueryAsync(decoratedLinks, request.Query, request.Trace);
                return Created("", result);
            }
            catch (Exception ex)
            {
                return BadRequest($"Error: {ex.Message}");
            }
        }

        [HttpPut]
        public async Task<IActionResult> UpdateLinks([FromBody] LinoRequest request)
        {
            if (string.IsNullOrWhiteSpace(request.Query))
            {
                return BadRequest("Query is required");
            }

            try
            {
                var decoratedLinks = new NamedLinksDecorator<uint>(_dbPath, request.Trace);
                var result = await ProcessLinoQueryAsync(decoratedLinks, request.Query, request.Trace);
                return Ok(result);
            }
            catch (Exception ex)
            {
                return BadRequest($"Error: {ex.Message}");
            }
        }

        [HttpDelete]
        public async Task<IActionResult> DeleteLinks([FromBody] LinoRequest request)
        {
            if (string.IsNullOrWhiteSpace(request.Query))
            {
                return BadRequest("Query is required");
            }

            try
            {
                var decoratedLinks = new NamedLinksDecorator<uint>(_dbPath, request.Trace);
                var result = await ProcessLinoQueryAsync(decoratedLinks, request.Query, request.Trace);
                return Ok(result);
            }
            catch (Exception ex)
            {
                return BadRequest($"Error: {ex.Message}");
            }
        }

        [HttpPost("query")]
        public async Task<IActionResult> ExecuteQuery([FromBody] LinoRequest request)
        {
            if (string.IsNullOrWhiteSpace(request.Query))
            {
                return BadRequest("Query is required");
            }

            try
            {
                var decoratedLinks = new NamedLinksDecorator<uint>(_dbPath, request.Trace);
                var result = await ProcessLinoQueryAsync(decoratedLinks, request.Query, request.Trace);
                return Ok(result);
            }
            catch (Exception ex)
            {
                return BadRequest($"Error: {ex.Message}");
            }
        }

        private async Task<LinoResponse> ProcessLinoQueryAsync(NamedLinksDecorator<uint> decoratedLinks, string query, bool trace)
        {
            var changesList = new List<(DoubletLink Before, DoubletLink After)>();
            var linksBeforeQuery = GetAllLinksAsString(decoratedLinks);
            
            var options = new QueryProcessor.Options
            {
                Query = query,
                Trace = trace,
                ChangesHandler = (beforeLink, afterLink) =>
                {
                    changesList.Add((new DoubletLink(beforeLink), new DoubletLink(afterLink)));
                    return decoratedLinks.Constants.Continue;
                }
            };

            // Execute the query
            await Task.Run(() => QueryProcessor.ProcessQuery(decoratedLinks, options));

            var linksAfterQuery = GetAllLinksAsString(decoratedLinks);
            var changes = FormatChanges(decoratedLinks, changesList);

            return new LinoResponse
            {
                Query = query,
                LinksBefore = linksBeforeQuery,
                LinksAfter = linksAfterQuery,
                Changes = changes,
                ChangeCount = changesList.Count
            };
        }

        private string GetAllLinksAsString(NamedLinksDecorator<uint> links)
        {
            var sb = new StringBuilder();
            var any = links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);

            links.Each(query, link =>
            {
                var formattedLink = links.Format(link);
                var namedLink = Namify(links, formattedLink);
                sb.AppendLine(namedLink);
                return links.Constants.Continue;
            });

            return sb.ToString().Trim();
        }

        private List<string> FormatChanges(NamedLinksDecorator<uint> links, List<(DoubletLink Before, DoubletLink After)> changesList)
        {
            var changes = new List<string>();
            
            foreach (var (linkBefore, linkAfter) in changesList)
            {
                var beforeText = linkBefore.IsNull() ? "" : links.Format(linkBefore);
                var afterText = linkAfter.IsNull() ? "" : links.Format(linkAfter);
                var formattedChange = $"({beforeText}) ({afterText})";
                changes.Add(Namify(links, formattedChange));
            }

            return changes;
        }

        private static string Namify(NamedLinksDecorator<uint> namedLinks, string linksNotation)
        {
            var numberGlobalRegex = new System.Text.RegularExpressions.Regex(@"\d+");
            var matches = numberGlobalRegex.Matches(linksNotation);
            var newLinksNotation = linksNotation;
            foreach (System.Text.RegularExpressions.Match match in matches)
            {
                var number = match.Value;
                var numberLink = uint.Parse(number);
                var name = namedLinks.GetName(numberLink);
                if (name != null)
                {
                    newLinksNotation = newLinksNotation.Replace(number, name);
                }
            }
            return newLinksNotation;
        }
    }

    public class LinoRequest
    {
        public string Query { get; set; } = "";
        public bool Trace { get; set; } = false;
    }

    public class LinoResponse
    {
        public string Query { get; set; } = "";
        public string LinksBefore { get; set; } = "";
        public string LinksAfter { get; set; } = "";
        public List<string> Changes { get; set; } = new();
        public int ChangeCount { get; set; } = 0;
    }
}