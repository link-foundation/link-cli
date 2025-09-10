using Grpc.Core;
using Foundation.Data.Doublets.Cli.Grpc;
using System.Diagnostics;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli
{
    public class LinoGrpcService : LinoService.LinoServiceBase
    {
        private const string DefaultDatabasePath = "db.links";

        public override async Task<LinoQueryResponse> ExecuteQuery(LinoQueryRequest request, ServerCallContext context)
        {
            var stopwatch = Stopwatch.StartNew();
            var response = new LinoQueryResponse();

            try
            {
                var dbPath = string.IsNullOrEmpty(request.DatabasePath) ? DefaultDatabasePath : request.DatabasePath;
                var decoratedLinks = new NamedLinksDecorator<uint>(dbPath, request.Trace);

                var changesList = new List<(DoubletLink Before, DoubletLink After)>();
                
                // Capture before state if requested
                if (request.IncludeBeforeState)
                {
                    response.BeforeState.AddRange(GetAllLinksAsStrings(decoratedLinks));
                }

                // Execute the query if provided
                if (!string.IsNullOrEmpty(request.Query))
                {
                    var options = new AdvancedMixedQueryProcessor.Options
                    {
                        Query = request.Query,
                        Trace = request.Trace,
                        ChangesHandler = (beforeLink, afterLink) =>
                        {
                            changesList.Add((new DoubletLink(beforeLink), new DoubletLink(afterLink)));
                            return decoratedLinks.Constants.Continue;
                        }
                    };

                    AdvancedMixedQueryProcessor.ProcessQuery(decoratedLinks, options);
                }

                // Process changes if requested
                if (request.IncludeChanges && changesList.Any())
                {
                    var simplifiedChanges = ChangesSimplifier.SimplifyChanges(changesList);
                    response.Changes.AddRange(simplifiedChanges.Select(change => 
                        FormatChange(decoratedLinks, change.Before, change.After)));
                }

                // Capture after state if requested
                if (request.IncludeAfterState)
                {
                    response.AfterState.AddRange(GetAllLinksAsStrings(decoratedLinks));
                }

                response.Success = true;
                response.Metadata = new ExecutionMetadata
                {
                    ExecutionTimeMs = stopwatch.ElapsedMilliseconds,
                    LinksProcessed = changesList.Count,
                    OperationsCount = 1,
                    Timestamp = DateTime.UtcNow.ToString("O")
                };
            }
            catch (Exception ex)
            {
                response.Success = false;
                response.ErrorMessage = ex.Message;
            }
            finally
            {
                stopwatch.Stop();
            }

            return await Task.FromResult(response);
        }

        public override async Task<LinoBatchResponse> ExecuteBatch(LinoBatchRequest request, ServerCallContext context)
        {
            var batchResponse = new LinoBatchResponse
            {
                Success = true
            };

            foreach (var queryRequest in request.Queries)
            {
                var queryResponse = await ExecuteQuery(queryRequest, context);
                batchResponse.Responses.Add(queryResponse);

                if (queryResponse.Success)
                {
                    batchResponse.SuccessfulOperations++;
                }
                else
                {
                    batchResponse.FailedOperations++;
                    if (request.StopOnError)
                    {
                        batchResponse.Success = false;
                        break;
                    }
                }
            }

            batchResponse.Success = batchResponse.FailedOperations == 0;
            return batchResponse;
        }

        public override async Task StreamQueries(IAsyncStreamReader<LinoStreamRequest> requestStream, 
            IServerStreamWriter<LinoStreamResponse> responseStream, ServerCallContext context)
        {
            try
            {
                await foreach (var request in requestStream.ReadAllAsync())
                {
                    switch (request.RequestTypeCase)
                    {
                        case LinoStreamRequest.RequestTypeOneofCase.Query:
                            var queryResponse = await ExecuteQuery(request.Query, context);
                            await responseStream.WriteAsync(new LinoStreamResponse
                            {
                                QueryResponse = queryResponse
                            });
                            break;

                        case LinoStreamRequest.RequestTypeOneofCase.Control:
                            await HandleControlMessage(request.Control, responseStream);
                            break;
                    }
                }
            }
            catch (Exception ex)
            {
                await responseStream.WriteAsync(new LinoStreamResponse
                {
                    Status = new StreamStatusMessage
                    {
                        Status = StreamStatusMessage.Types.Status.Error,
                        Message = ex.Message
                    }
                });
            }
        }

        public override async Task<GetAllLinksResponse> GetAllLinks(GetAllLinksRequest request, ServerCallContext context)
        {
            try
            {
                var dbPath = string.IsNullOrEmpty(request.DatabasePath) ? DefaultDatabasePath : request.DatabasePath;
                var decoratedLinks = new NamedLinksDecorator<uint>(dbPath, false);

                var links = GetAllLinksAsStrings(decoratedLinks, request.IncludeNames);

                return await Task.FromResult(new GetAllLinksResponse
                {
                    Success = true,
                    Links = { links }
                });
            }
            catch (Exception ex)
            {
                return await Task.FromResult(new GetAllLinksResponse
                {
                    Success = false,
                    ErrorMessage = ex.Message
                });
            }
        }

        public override async Task<GetStructureResponse> GetStructure(GetStructureRequest request, ServerCallContext context)
        {
            try
            {
                var dbPath = string.IsNullOrEmpty(request.DatabasePath) ? DefaultDatabasePath : request.DatabasePath;
                var decoratedLinks = new NamedLinksDecorator<uint>(dbPath, false);

                var linkId = request.LinkId;
                // TODO: Implement FormatStructure when available
                var result = $"Structure for link {linkId} (formatting not implemented yet)";

                return await Task.FromResult(new GetStructureResponse
                {
                    Success = true,
                    Structure = result
                });
            }
            catch (Exception ex)
            {
                return await Task.FromResult(new GetStructureResponse
                {
                    Success = false,
                    ErrorMessage = ex.Message
                });
            }
        }

        private static List<string> GetAllLinksAsStrings(NamedLinksDecorator<uint> links, bool includeNames = true)
        {
            var result = new List<string>();
            var any = links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);

            links.Each(query, link =>
            {
                // TODO: Implement Format when available  
                var doubletLink = new DoubletLink(link);
                var formattedLink = $"({doubletLink.Index}: {doubletLink.Source} {doubletLink.Target})";
                result.Add(formattedLink);
                return links.Constants.Continue;
            });

            return result;
        }

        private static string FormatChange(NamedLinksDecorator<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
        {
            // TODO: Implement Format when available
            var beforeText = linkBefore.IsNull() ? "" : $"({linkBefore.Index}: {linkBefore.Source} {linkBefore.Target})";
            var afterText = linkAfter.IsNull() ? "" : $"({linkAfter.Index}: {linkAfter.Source} {linkAfter.Target})";
            var formattedChange = $"({beforeText}) ({afterText})";
            return formattedChange;
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

        private static async Task HandleControlMessage(StreamControlMessage control, 
            IServerStreamWriter<LinoStreamResponse> responseStream)
        {
            var status = control.Type switch
            {
                StreamControlMessage.Types.ControlType.Pause => StreamStatusMessage.Types.Status.Paused,
                StreamControlMessage.Types.ControlType.Resume => StreamStatusMessage.Types.Status.Ready,
                StreamControlMessage.Types.ControlType.Cancel => StreamStatusMessage.Types.Status.Closed,
                StreamControlMessage.Types.ControlType.Ping => StreamStatusMessage.Types.Status.Ready,
                _ => StreamStatusMessage.Types.Status.Ready
            };

            await responseStream.WriteAsync(new LinoStreamResponse
            {
                Status = new StreamStatusMessage
                {
                    Status = status,
                    Message = $"Control message processed: {control.Type}"
                }
            });
        }
    }
}