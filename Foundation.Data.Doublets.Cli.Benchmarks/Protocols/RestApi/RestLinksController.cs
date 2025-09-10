using Microsoft.AspNetCore.Mvc;
using Foundation.Data.Doublets.Cli.Benchmarks.Models;
using Foundation.Data.Doublets.Cli.Benchmarks.Services;
using Foundation.Data.Doublets.Cli.Benchmarks.Serialization;

namespace Foundation.Data.Doublets.Cli.Benchmarks.Protocols.RestApi;

[ApiController]
[Route("api/[controller]")]
public class RestLinksController : ControllerBase
{
    private readonly ILinksService _linksService;

    public RestLinksController(ILinksService linksService)
    {
        _linksService = linksService;
    }

    [HttpPost]
    [Produces("text/plain")]
    [Consumes("text/plain")]
    public async Task<ActionResult<string>> CreateLink()
    {
        using var reader = new StreamReader(Request.Body);
        var linoString = await reader.ReadToEndAsync();
        
        // Parse LINO create request format: () ((source target))
        var request = ParseCreateRequest(linoString);
        if (request == null)
            return BadRequest("Invalid LINO format");

        var result = await _linksService.CreateLinkAsync(request);
        return Ok(LinoSerializer.SerializeLinkData(result));
    }

    [HttpGet("{id}")]
    [Produces("text/plain")]
    public async Task<ActionResult<string>> GetLink(uint id)
    {
        var result = await _linksService.GetLinkAsync(id);
        if (result == null)
            return NotFound();

        return Ok(LinoSerializer.SerializeLinkData(result));
    }

    [HttpPost("query")]
    [Produces("text/plain")]
    [Consumes("text/plain")]
    public async Task<ActionResult<string>> QueryLinks()
    {
        using var reader = new StreamReader(Request.Body);
        var linoString = await reader.ReadToEndAsync();
        
        var request = ParseQueryRequest(linoString);
        if (request == null)
            return BadRequest("Invalid LINO format");

        var results = await _linksService.QueryLinksAsync(request);
        return Ok(LinoSerializer.SerializeLinkDataCollection(results));
    }

    [HttpPut]
    [Produces("text/plain")]
    [Consumes("text/plain")]
    public async Task<ActionResult<string>> UpdateLink()
    {
        using var reader = new StreamReader(Request.Body);
        var linoString = await reader.ReadToEndAsync();
        
        var request = ParseUpdateRequest(linoString);
        if (request == null)
            return BadRequest("Invalid LINO format");

        try
        {
            var result = await _linksService.UpdateLinkAsync(request);
            return Ok(LinoSerializer.SerializeLinkData(result));
        }
        catch (InvalidOperationException)
        {
            return NotFound();
        }
    }

    [HttpDelete]
    [Consumes("text/plain")]
    public async Task<ActionResult> DeleteLink()
    {
        using var reader = new StreamReader(Request.Body);
        var linoString = await reader.ReadToEndAsync();
        
        var request = ParseDeleteRequest(linoString);
        if (request == null)
            return BadRequest("Invalid LINO format");

        var success = await _linksService.DeleteLinkAsync(request);
        return success ? NoContent() : NotFound();
    }

    private static CreateLinkRequest? ParseCreateRequest(string linoString)
    {
        // Expected format: () ((source target))
        try
        {
            var parts = linoString.Trim().Split(new[] { "()", "((" }, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length < 1) return null;
            
            var linkPart = parts[0].Replace("))", "").Trim();
            var values = linkPart.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            
            if (values.Length != 2) return null;
            
            return new CreateLinkRequest
            {
                Source = uint.Parse(values[0]),
                Target = uint.Parse(values[1])
            };
        }
        catch
        {
            return null;
        }
    }

    private static UpdateLinkRequest? ParseUpdateRequest(string linoString)
    {
        // Expected format: ((id: * *)) ((id: source target))
        try
        {
            var parts = linoString.Split(new[] { "))" }, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length < 2) return null;
            
            var afterPart = parts[1].Trim().TrimStart('(').TrimStart('(');
            var values = afterPart.Split(new[] { ':', ' ' }, StringSplitOptions.RemoveEmptyEntries);
            
            if (values.Length != 3) return null;
            
            return new UpdateLinkRequest
            {
                Id = uint.Parse(values[0]),
                Source = uint.Parse(values[1]),
                Target = uint.Parse(values[2])
            };
        }
        catch
        {
            return null;
        }
    }

    private static DeleteLinkRequest? ParseDeleteRequest(string linoString)
    {
        // Expected format: ((id: * *)) ()
        try
        {
            var parts = linoString.Split(new[] { "((", ": * *))" }, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length < 2) return null;
            
            return new DeleteLinkRequest
            {
                Id = uint.Parse(parts[1].Trim())
            };
        }
        catch
        {
            return null;
        }
    }

    private static QueryLinksRequest? ParseQueryRequest(string linoString)
    {
        // Expected format: ((id: source target)) ((id: source target))
        try
        {
            var firstPart = linoString.Split(new[] { "))" }, StringSplitOptions.RemoveEmptyEntries)[0];
            firstPart = firstPart.Trim().TrimStart('(').TrimStart('(');
            
            var parts = firstPart.Split(new[] { ':', ' ' }, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length != 3) return null;
            
            return new QueryLinksRequest
            {
                Id = parts[0] == "*" ? null : uint.Parse(parts[0]),
                Source = parts[1] == "*" ? null : uint.Parse(parts[1]),
                Target = parts[2] == "*" ? null : uint.Parse(parts[2])
            };
        }
        catch
        {
            return null;
        }
    }
}