using System.Net.WebSockets;
using System.Text;

Console.WriteLine("Testing LiNo WebSocket server...");

using var client = new ClientWebSocket();

try
{
    await client.ConnectAsync(new Uri("ws://localhost:8081/ws"), CancellationToken.None);
    Console.WriteLine("Connected to WebSocket server");

    // Test query: Create a link
    var testQuery = "() ((1 1))";
    var queryBytes = Encoding.UTF8.GetBytes(testQuery);
    
    await client.SendAsync(new ArraySegment<byte>(queryBytes), WebSocketMessageType.Text, true, CancellationToken.None);
    Console.WriteLine($"Sent query: {testQuery}");

    // Receive response
    var buffer = new byte[1024 * 4];
    var result = await client.ReceiveAsync(new ArraySegment<byte>(buffer), CancellationToken.None);
    
    if (result.MessageType == WebSocketMessageType.Text)
    {
        var response = Encoding.UTF8.GetString(buffer, 0, result.Count);
        Console.WriteLine($"Received response: {response}");
    }

    await client.CloseAsync(WebSocketCloseStatus.NormalClosure, "Test completed", CancellationToken.None);
    Console.WriteLine("WebSocket test completed successfully");
}
catch (Exception ex)
{
    Console.WriteLine($"Error: {ex.Message}");
}