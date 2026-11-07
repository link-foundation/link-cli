using System.Collections.Concurrent;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>
/// HTTP and WebSocket transport of the GraphQL layer. Connections have threads; requests run on the serving thread
/// through a queue, so the store needs no locks. After each write every subscription runs again, and is sent if its
/// answer changed. WebSocket speaks the subprotocols <c>graphql-ws</c> (<c>subscriptions-transport-ws</c>, which
/// <c>DeepClient</c> uses) and <c>graphql-transport-ws</c> (the <c>graphql-ws</c> library).
/// </summary>
public sealed class GraphqlServer : IDisposable
{
    private const int MaxBody = 64 * 1024 * 1024;
    private const int JobStack = 256 * 1024 * 1024;
    private const string Cors = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\n";

    /// <summary>Headers allowed when a preflight does not name any; Hasura echoes the ones it is asked about.</summary>
    private const string CorsHeaders = "authorization, content-type, x-hasura-admin-secret, x-hasura-client, x-hasura-role, x-hasura-user-id";

    private readonly TcpListener _listener;
    private readonly BlockingCollection<Job> _jobs = new();
    private volatile bool _stopped;

    public GraphqlServer(IPEndPoint address)
    {
        _listener = new TcpListener(address);
        _listener.Start();
    }

    public IPEndPoint LocalEndpoint => (IPEndPoint)_listener.LocalEndpoint;

    /// <summary>Serves until <see cref="Stop"/>.</summary>
    public void Serve(DeepStore store, Settings settings)
    {
        var accepting = new Thread(() => Accept(settings)) { IsBackground = true };
        accepting.Start();
        Exception? failure = null;
        // Deep's trees are walked recursively on this thread.
        var serving = new Thread(
            () =>
            {
                try
                {
                    RunJobs(store, settings);
                }
                catch (Exception error)
                {
                    failure = error;
                }
            },
            JobStack);
        serving.Start();
        serving.Join();
        if (failure is not null)
        {
            throw new InvalidOperationException(failure.Message, failure);
        }
    }

    /// <summary>Stops accepting and ends <see cref="Serve"/>.</summary>
    public void Stop()
    {
        _stopped = true;
        _listener.Stop();
        _jobs.CompleteAdding();
    }

    public void Dispose()
    {
        Stop();
        _listener.Dispose();
        _jobs.Dispose();
    }

    private void Accept(Settings settings)
    {
        long next = 0;
        while (!_stopped)
        {
            TcpClient client;
            try
            {
                client = _listener.AcceptTcpClient();
            }
            catch (Exception error) when (error is SocketException || _stopped)
            {
                // Stopping can surface as any of several errors, depending on where it finds this call.
                continue;
            }
            var connection = ++next;
            new Thread(() => Connection(client, settings, connection)) { IsBackground = true }.Start();
        }
    }

    private void Connection(TcpClient client, Settings settings, long connection)
    {
        try
        {
            using (client)
            {
                client.NoDelay = true;
                Handle(client, settings, connection);
            }
        }
        catch (Exception error) when (error is IOException or SocketException or ObjectDisposedException or InvalidOperationException)
        {
            // The peer went away, or the server stopped.
        }
    }

    private static (string Query, string? Operation, Json Variables) RequestParts(Json body) => (
        body.Get("query")?.AsStr() ?? "",
        body.Get("operationName")?.AsStr(),
        body.Get("variables") ?? Json.Null);

    private void RunJobs(DeepStore store, Settings settings)
    {
        var subscriptions = new List<Subscription>();
        foreach (var job in _jobs.GetConsumingEnumerable())
        {
            switch (job)
            {
                case HttpJob http:
                    {
                        var (query, operation, variables) = RequestParts(http.Body);
                        var before = store.Generation;
                        var (response, _) = Exec.Execute(store, settings, query, operation, variables, http.Auth);
                        http.Reply.TrySetResult(response);
                        if (store.Generation != before)
                        {
                            Refresh(store, settings, subscriptions);
                        }
                        break;
                    }
                case SubscribeJob subscribe:
                    {
                        var (query, operation, variables) = RequestParts(subscribe.Payload);
                        var before = store.Generation;
                        var (response, _) = Exec.Execute(store, settings, query, operation, variables, subscribe.Auth);
                        if (response.Get("errors") is not null)
                        {
                            Quietly(() => SendError(subscribe.Socket, subscribe.Id, response, subscribe.Legacy));
                        }
                        else
                        {
                            var last = response.Write();
                            Quietly(() => SendData(subscribe.Socket, subscribe.Id, last, subscribe.Legacy));
                            subscriptions.Add(new Subscription(subscribe, query, operation, variables, last));
                        }
                        if (store.Generation != before)
                        {
                            Refresh(store, settings, subscriptions);
                        }
                        break;
                    }
                case StopJob stop:
                    subscriptions.RemoveAll(each =>
                        each.Job.Connection == stop.Connection && (stop.Id is null || stop.Id == each.Job.Id));
                    break;
            }
        }
    }

    private static bool Quietly(Action send)
    {
        try
        {
            send();
            return true;
        }
        catch (Exception error) when (error is IOException or SocketException or ObjectDisposedException or InvalidOperationException)
        {
            return false;
        }
    }

    private static void Refresh(DeepStore store, Settings settings, List<Subscription> subscriptions)
    {
        subscriptions.RemoveAll(each =>
        {
            var (response, _) = Exec.Execute(store, settings, each.Query, each.Operation, each.Variables, each.Job.Auth);
            var text = response.Write();
            if (text == each.Last)
            {
                return false;
            }
            each.Last = text;
            return !Quietly(() => SendData(each.Job.Socket, each.Job.Id, text, each.Job.Legacy));
        });
    }

    private static string Message(string kind, string? id = null, string? payload = null)
    {
        var idPart = id is null ? "" : ",\"id\":" + Json.Str(id).Write();
        var payloadPart = payload is null ? "" : ",\"payload\":" + payload;
        return "{\"type\":\"" + kind + "\"" + idPart + payloadPart + "}";
    }

    private static void SendText(SharedSocket socket, string text) => SendFrame(socket, 1, Encoding.UTF8.GetBytes(text));

    private static void SendData(SharedSocket socket, string id, string payload, bool legacy) =>
        SendText(socket, Message(legacy ? "data" : "next", id, payload));

    private static void SendError(SharedSocket socket, string id, Json response, bool legacy)
    {
        if (legacy)
        {
            SendData(socket, id, response.Write(), true);
            return;
        }
        SendText(socket, Message("error", id, response.Get("errors")?.Write() ?? "[]"));
    }

    private static void SendFrame(SharedSocket socket, byte opcode, byte[] payload)
    {
        using var frame = new MemoryStream(payload.Length + 10);
        frame.WriteByte((byte)(0x80 | opcode));
        if (payload.Length <= 125)
        {
            frame.WriteByte((byte)payload.Length);
        }
        else if (payload.Length <= 65535)
        {
            frame.WriteByte(126);
            frame.WriteByte((byte)(payload.Length >> 8));
            frame.WriteByte((byte)payload.Length);
        }
        else
        {
            frame.WriteByte(127);
            for (var shift = 56; shift >= 0; shift -= 8)
            {
                frame.WriteByte((byte)((long)payload.Length >> shift));
            }
        }
        frame.Write(payload);
        lock (socket)
        {
            socket.Stream.Write(frame.GetBuffer(), 0, (int)frame.Length);
        }
    }

    private static bool ReadExact(Stream reader, byte[] buffer)
    {
        try
        {
            reader.ReadExactly(buffer);
            return true;
        }
        catch (EndOfStreamException)
        {
            return false;
        }
    }

    /// <summary>Reads one WebSocket message, joining fragments and answering pings; null when the connection is done.</summary>
    private static string? ReadMessage(Stream reader, SharedSocket socket)
    {
        using var text = new MemoryStream();
        var head = new byte[2];
        while (true)
        {
            if (!ReadExact(reader, head))
            {
                return null;
            }
            var opcode = head[0] & 0x0f;
            var length = (ulong)(head[1] & 0x7f);
            if (length is 126 or 127)
            {
                var bytes = new byte[length == 126 ? 2 : 8];
                if (!ReadExact(reader, bytes))
                {
                    return null;
                }
                length = bytes.Aggregate(0UL, (word, each) => (word << 8) | each);
            }
            if (length > MaxBody)
            {
                return null;
            }
            var mask = new byte[4];
            if ((head[1] & 0x80) != 0 && !ReadExact(reader, mask))
            {
                return null;
            }
            var payload = new byte[(int)length];
            if (!ReadExact(reader, payload))
            {
                return null;
            }
            for (var at = 0; at < payload.Length; at++)
            {
                payload[at] ^= mask[at % 4];
            }
            switch (opcode)
            {
                case 8:
                    Quietly(() => SendFrame(socket, 8, payload));
                    return null;
                case 9:
                    SendFrame(socket, 10, payload);
                    break;
                case 10:
                    break;
                default:
                    text.Write(payload);
                    if ((head[0] & 0x80) != 0)
                    {
                        return Encoding.UTF8.GetString(text.GetBuffer(), 0, (int)text.Length);
                    }
                    break;
            }
        }
    }

    /// <summary>One line of the request head, without its end; null at the end of the stream.</summary>
    private static string? ReadLine(Stream reader)
    {
        using var line = new MemoryStream();
        while (true)
        {
            var next = reader.ReadByte();
            if (next < 0)
            {
                return null;
            }
            if (next == '\n')
            {
                return Encoding.UTF8.GetString(line.GetBuffer(), 0, (int)line.Length).TrimEnd();
            }
            line.WriteByte((byte)next);
        }
    }

    private static void WriteText(Stream writer, string text)
    {
        var bytes = Encoding.UTF8.GetBytes(text);
        writer.Write(bytes);
    }

    private static void Respond(Stream writer, string status, string body)
    {
        var bytes = Encoding.UTF8.GetBytes(body);
        WriteText(writer, $"HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\n{Cors}Access-Control-Allow-Headers: {CorsHeaders}\r\nContent-Length: {bytes.Length}\r\n\r\n{body}");
    }

    private void Handle(TcpClient client, Settings settings, long connection)
    {
        var writer = client.GetStream();
        using var reader = new BufferedStream(writer);
        while (true)
        {
            var requestLine = ReadLine(reader);
            if (requestLine is null)
            {
                return;
            }
            var words = requestLine.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
            var method = words.Length > 0 ? words[0] : "";
            var path = (words.Length > 1 ? words[1] : "").Split('?')[0];
            var headers = new Dictionary<string, string>();
            while (true)
            {
                var line = ReadLine(reader);
                if (line is null)
                {
                    return;
                }
                if (line.Length == 0)
                {
                    break;
                }
                var colon = line.IndexOf(':');
                if (colon >= 0)
                {
                    headers[line[..colon].Trim().ToLowerInvariant()] = line[(colon + 1)..].Trim();
                }
            }
            var graphql = path is "/v1/graphql" or "/gql" or "/v1alpha1/graphql";
            if (graphql && string.Equals(headers.GetValueOrDefault("upgrade"), "websocket", StringComparison.OrdinalIgnoreCase))
            {
                WebSocket(reader, writer, headers, settings, connection);
                return;
            }
            var length = ulong.TryParse(headers.GetValueOrDefault("content-length"), NumberStyles.None, CultureInfo.InvariantCulture, out var declared)
                ? declared
                : 0;
            if (length > MaxBody)
            {
                Respond(writer, "413 Payload Too Large", "{}");
                return;
            }
            var body = new byte[(int)length];
            if (!ReadExact(reader, body))
            {
                return;
            }
            switch (method, path)
            {
                case ("OPTIONS", _):
                    WriteText(writer, $"HTTP/1.1 204 No Content\r\n{Cors}Access-Control-Allow-Headers: {headers.GetValueOrDefault("access-control-request-headers", CorsHeaders)}\r\nContent-Length: 0\r\n\r\n");
                    break;
                case ("GET", "/healthz" or "/api/healthz" or "/v1/healthz"):
                    Respond(writer, "200 OK", "{\"status\":\"ok\"}");
                    break;
                case ("GET", "/v1/version" or "/v1/version/"):
                    Respond(writer, "200 OK", "{\"version\":\"clink\"}");
                    break;
                case ("POST", _) when graphql:
                    Respond(writer, "200 OK", Answer(settings, headers, body).Write());
                    break;
                default:
                    Respond(writer, "404 Not Found", "{\"error\":\"resource does not exist\"}");
                    break;
            }
        }
    }

    private Json Answer(Settings settings, Dictionary<string, string> headers, byte[] body)
    {
        Auth auth;
        try
        {
            auth = settings.Authorize(headers.GetValueOrDefault("authorization"), headers.GetValueOrDefault("x-hasura-admin-secret"));
        }
        catch (FormatException error)
        {
            return new GqlError(error.Message, "invalid-jwt", "$").ToJson();
        }
        Json request;
        try
        {
            request = Json.Parse(Encoding.UTF8.GetString(body));
        }
        catch (FormatException error)
        {
            return GqlError.Validation(error.Message, "$").ToJson();
        }
        var reply = new TaskCompletionSource<Json>(TaskCreationOptions.RunContinuationsAsynchronously);
        _jobs.Add(new HttpJob(request, auth, reply));
        return reply.Task.GetAwaiter().GetResult();
    }

    private void WebSocket(Stream reader, Stream writer, Dictionary<string, string> headers, Settings settings, long connection)
    {
        var modern = headers.GetValueOrDefault("sec-websocket-protocol", "").Split(',').Any(name => name.Trim() == "graphql-transport-ws");
        var legacy = !modern;
        WriteText(writer, $"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {Crypto.WebSocketAccept(headers.GetValueOrDefault("sec-websocket-key", ""))}\r\nSec-WebSocket-Protocol: {(modern ? "graphql-transport-ws" : "graphql-ws")}\r\n\r\n");
        var socket = new SharedSocket(writer);
        Auth auth;
        try
        {
            auth = settings.Authorize(headers.GetValueOrDefault("authorization"), headers.GetValueOrDefault("x-hasura-admin-secret"));
        }
        catch (FormatException)
        {
            auth = default;
        }
        try
        {
            while (ReadMessage(reader, socket) is { } text)
            {
                Json incoming;
                try
                {
                    incoming = Json.Parse(text);
                }
                catch (FormatException)
                {
                    continue;
                }
                var id = incoming.Get("id")?.AsStr();
                var payload = incoming.Get("payload") ?? Json.Null;
                switch (incoming.Get("type")?.AsStr() ?? "")
                {
                    case "connection_init":
                        var holder = payload.Get("headers") ?? payload;
                        var authorization = (holder.Get("Authorization") ?? holder.Get("authorization"))?.AsStr();
                        var admin = holder.Get("x-hasura-admin-secret")?.AsStr();
                        try
                        {
                            var found = settings.Authorize(authorization, admin);
                            if (authorization is not null || admin is not null)
                            {
                                auth = found;
                            }
                        }
                        catch (FormatException problem)
                        {
                            SendText(socket, Message("connection_error", null, Json.Object(("message", Json.Str(problem.Message))).Write()));
                            return;
                        }
                        SendText(socket, Message("connection_ack"));
                        if (legacy)
                        {
                            SendText(socket, Message("ka"));
                        }
                        break;
                    case "start" or "subscribe":
                        _jobs.Add(new SubscribeJob(connection, id ?? "", payload, auth, legacy, socket));
                        break;
                    case "stop" or "complete":
                        _jobs.Add(new StopJob(connection, id));
                        if (legacy)
                        {
                            SendText(socket, Message("complete", id));
                        }
                        break;
                    case "ping":
                        SendText(socket, Message("pong"));
                        break;
                    case "connection_terminate":
                        return;
                }
            }
        }
        finally
        {
            if (!_stopped)
            {
                _jobs.Add(new StopJob(connection, null));
            }
        }
    }

    private sealed class SharedSocket(Stream stream)
    {
        public Stream Stream { get; } = stream;
    }

    private abstract record Job;

    private sealed record HttpJob(Json Body, Auth Auth, TaskCompletionSource<Json> Reply) : Job;

    private sealed record SubscribeJob(long Connection, string Id, Json Payload, Auth Auth, bool Legacy, SharedSocket Socket) : Job;

    private sealed record StopJob(long Connection, string? Id) : Job;

    private sealed class Subscription(SubscribeJob job, string query, string? operation, Json variables, string last)
    {
        public SubscribeJob Job { get; } = job;
        public string Query { get; } = query;
        public string? Operation { get; } = operation;
        public Json Variables { get; } = variables;
        public string Last { get; set; } = last;
    }
}
