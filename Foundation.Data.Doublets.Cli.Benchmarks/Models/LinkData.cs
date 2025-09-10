using System.ComponentModel.DataAnnotations;

namespace Foundation.Data.Doublets.Cli.Benchmarks.Models;

public class LinkData
{
    public uint Id { get; set; }
    public uint Source { get; set; }
    public uint Target { get; set; }
}

public class CreateLinkRequest
{
    [Required]
    public uint Source { get; set; }
    
    [Required]
    public uint Target { get; set; }
}

public class UpdateLinkRequest
{
    [Required]
    public uint Id { get; set; }
    
    [Required]
    public uint Source { get; set; }
    
    [Required]
    public uint Target { get; set; }
}

public class DeleteLinkRequest
{
    [Required]
    public uint Id { get; set; }
}

public class QueryLinksRequest
{
    public uint? Id { get; set; }
    public uint? Source { get; set; }
    public uint? Target { get; set; }
}