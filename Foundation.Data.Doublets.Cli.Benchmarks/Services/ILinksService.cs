using Foundation.Data.Doublets.Cli.Benchmarks.Models;

namespace Foundation.Data.Doublets.Cli.Benchmarks.Services;

public interface ILinksService
{
    Task<LinkData> CreateLinkAsync(CreateLinkRequest request);
    Task<LinkData?> GetLinkAsync(uint id);
    Task<IEnumerable<LinkData>> QueryLinksAsync(QueryLinksRequest request);
    Task<LinkData> UpdateLinkAsync(UpdateLinkRequest request);
    Task<bool> DeleteLinkAsync(DeleteLinkRequest request);
}