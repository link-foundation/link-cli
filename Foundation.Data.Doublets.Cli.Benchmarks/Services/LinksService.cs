using Foundation.Data.Doublets.Cli.Benchmarks.Models;
using Foundation.Data.Doublets.Cli;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;

namespace Foundation.Data.Doublets.Cli.Benchmarks.Services;

public class LinksService : ILinksService, IDisposable
{
    private readonly NamedLinksDecorator<uint> _links;
    private readonly string _databasePath;

    public LinksService(string databasePath = "benchmark.links")
    {
        _databasePath = databasePath;
        _links = new NamedLinksDecorator<uint>(databasePath, false);
    }

    public async Task<LinkData> CreateLinkAsync(CreateLinkRequest request)
    {
        return await Task.Run(() =>
        {
            var linkId = _links.GetOrCreate(request.Source, request.Target);
            return new LinkData
            {
                Id = linkId,
                Source = request.Source,
                Target = request.Target
            };
        });
    }

    public async Task<LinkData?> GetLinkAsync(uint id)
    {
        return await Task.Run(() =>
        {
            var any = _links.Constants.Any;
            var results = new List<LinkData>();
            
            _links.Each(new List<uint> { id, any, any }, link =>
            {
                results.Add(new LinkData
                {
                    Id = link[0],
                    Source = link[1],
                    Target = link[2]
                });
                return _links.Constants.Continue;
            });

            return results.FirstOrDefault();
        });
    }

    public async Task<IEnumerable<LinkData>> QueryLinksAsync(QueryLinksRequest request)
    {
        return await Task.Run(() =>
        {
            var results = new List<LinkData>();
            var any = _links.Constants.Any;

            var queryLink = new List<uint>
            {
                request.Id ?? any,
                request.Source ?? any,
                request.Target ?? any
            };

            _links.Each(queryLink, link =>
            {
                results.Add(new LinkData
                {
                    Id = link[0],
                    Source = link[1],
                    Target = link[2]
                });
                return _links.Constants.Continue;
            });

            return results;
        });
    }

    public async Task<LinkData> UpdateLinkAsync(UpdateLinkRequest request)
    {
        return await Task.Run(() =>
        {
            var any = _links.Constants.Any;
            var existingLink = new List<uint> { request.Id, any, any };
            var newLink = new List<uint> { request.Id, request.Source, request.Target };
            
            _links.Update(existingLink, newLink, null);
            
            return new LinkData
            {
                Id = request.Id,
                Source = request.Source,
                Target = request.Target
            };
        });
    }

    public async Task<bool> DeleteLinkAsync(DeleteLinkRequest request)
    {
        return await Task.Run(() =>
        {
            try
            {
                var any = _links.Constants.Any;
                var linkToDelete = new List<uint> { request.Id, any, any };
                _links.Delete(linkToDelete, null);
                return true;
            }
            catch
            {
                return false;
            }
        });
    }

    public void Dispose()
    {
        // NamedLinksDecorator doesn't implement IDisposable, but the underlying links do
        GC.SuppressFinalize(this);
    }
}