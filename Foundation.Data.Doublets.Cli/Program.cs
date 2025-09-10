using System.CommandLine;
using System.CommandLine.Invocation;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;

using static Foundation.Data.Doublets.Cli.ChangesSimplifier;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using QueryProcessor = Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;
using Foundation.Data.Doublets.Cli;
using System.Text.RegularExpressions;

const string defaultDatabaseFilename = "db.links";

var dbOption = new Option<string>(
  name: "--db",
  description: "Path to the links database file",
  getDefaultValue: () => defaultDatabaseFilename
);
dbOption.AddAlias("--data-source");
dbOption.AddAlias("--data");
dbOption.AddAlias("-d");

var queryOption = new Option<string>(
  name: "--query",
  description: "LiNo query for CRUD operation"
);
queryOption.AddAlias("--apply");
queryOption.AddAlias("--do");
queryOption.AddAlias("-q");

var queryArgument = new Argument<string>(
  name: "query",
  description: "LiNo query for CRUD operation"
);
queryArgument.Arity = ArgumentArity.ZeroOrOne;

var traceOption = new Option<bool>(
  name: "--trace",
  description: "Enable trace (verbose output)",
  getDefaultValue: () => false
);
traceOption.AddAlias("-t");

var structureOption = new Option<uint?>(
  name: "--structure",
  description: "ID of the link to format its structure"
);
structureOption.AddAlias("-s");

var beforeOption = new Option<bool>(
  name: "--before",
  description: "Print the state of the database before applying changes",
  getDefaultValue: () => false
);
beforeOption.AddAlias("-b");

var changesOption = new Option<bool>(
  name: "--changes",
  description: "Print the changes applied by the query",
  getDefaultValue: () => false
);
changesOption.AddAlias("-c");

var afterOption = new Option<bool>(
  name: "--after",
  description: "Print the state of the database after applying changes",
  getDefaultValue: () => false
);
afterOption.AddAlias("--links");
afterOption.AddAlias("-a");

var serverOption = new Option<bool>(
  name: "--server",
  description: "Start server listening on a port",
  getDefaultValue: () => false
);

var portOption = new Option<int>(
  name: "--port",
  description: "Port to listen on when in server mode",
  getDefaultValue: () => 8080
);
portOption.AddAlias("-p");

var rootCommand = new RootCommand("LiNo CLI Tool for managing links data store")
{
  dbOption,
  queryOption,
  queryArgument,
  traceOption,
  structureOption,
  beforeOption,
  changesOption,
  afterOption,
  serverOption,
  portOption
};

rootCommand.SetHandler(async (context) =>
{
  var db = context.ParseResult.GetValueForOption(dbOption)!;
  var queryOptionValue = context.ParseResult.GetValueForOption(queryOption);
  var queryArgumentValue = context.ParseResult.GetValueForArgument(queryArgument);
  var trace = context.ParseResult.GetValueForOption(traceOption);
  var structure = context.ParseResult.GetValueForOption(structureOption);
  var before = context.ParseResult.GetValueForOption(beforeOption);
  var changes = context.ParseResult.GetValueForOption(changesOption);
  var after = context.ParseResult.GetValueForOption(afterOption);
  var server = context.ParseResult.GetValueForOption(serverOption);
  var port = context.ParseResult.GetValueForOption(portOption);

  // Handle server mode
  if (server)
  {
    await StartServer(db, port, trace);
    return;
  }

  var decoratedLinks = new NamedLinksDecorator<uint>(db, trace);

  // If --structure is provided, handle it separately
  if (structure.HasValue)
  {
    var linkId = structure.Value;
    try
    {
      var structureFormatted = decoratedLinks.FormatStructure(linkId, link => decoratedLinks.IsFullPoint(linkId), true, true);
      Console.WriteLine(Namify(decoratedLinks, structureFormatted));
    }
    catch (Exception ex)
    {
      Console.Error.WriteLine($"Error formatting structure for link ID {linkId}: {ex.Message}");
      context.ExitCode = 1;
      return;
    }
    return; // Exit after handling --structure
  }

  if (before)
  {
    PrintAllLinks(decoratedLinks);
  }

  var effectiveQuery = !string.IsNullOrWhiteSpace(queryOptionValue) ? queryOptionValue : queryArgumentValue;

  var changesList = new List<(DoubletLink Before, DoubletLink After)>();

  if (!string.IsNullOrWhiteSpace(effectiveQuery))
  {
    var options = new QueryProcessor.Options
    {
      Query = effectiveQuery,
      Trace = trace,
      ChangesHandler = (beforeLink, afterLink) =>
      {
        changesList.Add((new DoubletLink(beforeLink), new DoubletLink(afterLink)));
        return decoratedLinks.Constants.Continue;
      }
    };

    QueryProcessor.ProcessQuery(decoratedLinks, options);
  }

  if (changes && changesList.Any())
  {
    // Simplify the collected changes
    var simplifiedChanges = SimplifyChanges(changesList);

    // Print the simplified changes
    foreach (var (linkBefore, linkAfter) in simplifiedChanges)
    {
      PrintChange(decoratedLinks, linkBefore, linkAfter);
    }
  }

  if (after)
  {
    PrintAllLinks(decoratedLinks);
  }
});

await rootCommand.InvokeAsync(args);

static string Namify(NamedLinksDecorator<uint> namedLinks, string linksNotation)
{
  var numberGlobalRegex = new Regex(@"\d+");
  var matches = numberGlobalRegex.Matches(linksNotation);
  var newLinksNotation = linksNotation;
  foreach (Match match in matches)
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

static void PrintAllLinks(NamedLinksDecorator<uint> links)
{
  var any = links.Constants.Any;
  var query = new DoubletLink(index: any, source: any, target: any);

  links.Each(query, link =>
  {
    var formattedLink = links.Format(link);
    Console.WriteLine(Namify(links, formattedLink));
    return links.Constants.Continue;
  });
}

static void PrintChange(NamedLinksDecorator<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
{
  var beforeText = linkBefore.IsNull() ? "" : links.Format(linkBefore);
  var afterText = linkAfter.IsNull() ? "" : links.Format(linkAfter);
  var formattedChange = $"({beforeText}) ({afterText})";
  Console.WriteLine(Namify(links, formattedChange));
}

static async Task StartServer(string db, int port, bool trace)
{
  var builder = WebApplication.CreateSlimBuilder();
  
  builder.Services.AddSingleton(provider => new NamedLinksDecorator<uint>(db, trace));
  
  var app = builder.Build();
  
  app.UseWebSockets();
  
  app.Map("/ws", HandleWebSocketEndpoint);
  
  Console.WriteLine($"LiNo WebSocket server started on ws://localhost:{port}/ws");
  Console.WriteLine("Press Ctrl+C to stop the server");
  
  await app.RunAsync($"http://localhost:{port}");
}

static async Task HandleWebSocketEndpoint(HttpContext context)
{
  if (!context.WebSockets.IsWebSocketRequest)
  {
    context.Response.StatusCode = 400;
    return;
  }
  
  var decoratedLinks = context.RequestServices.GetRequiredService<NamedLinksDecorator<uint>>();
  using var webSocket = await context.WebSockets.AcceptWebSocketAsync();
  await HandleWebSocketConnection(webSocket, decoratedLinks);
}

static async Task HandleWebSocketConnection(WebSocket webSocket, NamedLinksDecorator<uint> decoratedLinks)
{
  var buffer = new byte[1024 * 4];
  
  while (webSocket.State == WebSocketState.Open)
  {
    try
    {
      var result = await webSocket.ReceiveAsync(new ArraySegment<byte>(buffer), CancellationToken.None);
      
      if (result.MessageType == WebSocketMessageType.Close)
      {
        await webSocket.CloseAsync(WebSocketCloseStatus.NormalClosure, "", CancellationToken.None);
        break;
      }
      
      if (result.MessageType == WebSocketMessageType.Text)
      {
        var query = Encoding.UTF8.GetString(buffer, 0, result.Count);
        var response = await ProcessLinoQuery(query, decoratedLinks);
        
        var responseBytes = Encoding.UTF8.GetBytes(response);
        await webSocket.SendAsync(
          new ArraySegment<byte>(responseBytes),
          WebSocketMessageType.Text,
          true,
          CancellationToken.None
        );
      }
    }
    catch (WebSocketException ex)
    {
      Console.WriteLine($"WebSocket error: {ex.Message}");
      break;
    }
    catch (Exception ex)
    {
      Console.WriteLine($"Error processing request: {ex.Message}");
      var errorResponse = JsonSerializer.Serialize(new { error = ex.Message });
      var errorBytes = Encoding.UTF8.GetBytes(errorResponse);
      
      try
      {
        await webSocket.SendAsync(
          new ArraySegment<byte>(errorBytes),
          WebSocketMessageType.Text,
          true,
          CancellationToken.None
        );
      }
      catch (WebSocketException)
      {
        break;
      }
    }
  }
}

static async Task<string> ProcessLinoQuery(string query, NamedLinksDecorator<uint> decoratedLinks)
{
  return await Task.Run(() =>
  {
    try
    {
      var changesList = new List<(DoubletLink Before, DoubletLink After)>();
      var results = new List<string>();
      
      if (!string.IsNullOrWhiteSpace(query))
      {
        var options = new QueryProcessor.Options
        {
          Query = query,
          Trace = false,
          ChangesHandler = (beforeLink, afterLink) =>
          {
            changesList.Add((new DoubletLink(beforeLink), new DoubletLink(afterLink)));
            return decoratedLinks.Constants.Continue;
          }
        };
        
        QueryProcessor.ProcessQuery(decoratedLinks, options);
        
        // Collect current state of all links
        var any = decoratedLinks.Constants.Any;
        var linkQuery = new DoubletLink(index: any, source: any, target: any);
        
        decoratedLinks.Each(linkQuery, link =>
        {
          var formattedLink = decoratedLinks.Format(link);
          results.Add(Namify(decoratedLinks, formattedLink));
          return decoratedLinks.Constants.Continue;
        });
      }
      
      return JsonSerializer.Serialize(new
      {
        query,
        changes = changesList.Select(change => new
        {
          before = change.Before.IsNull() ? null : decoratedLinks.Format(change.Before),
          after = change.After.IsNull() ? null : decoratedLinks.Format(change.After)
        }),
        links = results
      });
    }
    catch (Exception ex)
    {
      return JsonSerializer.Serialize(new { error = ex.Message });
    }
  });
}