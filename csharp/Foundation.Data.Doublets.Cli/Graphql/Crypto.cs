using System.Globalization;
using System.Security.Cryptography;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>What the wire needs of cryptography: the WebSocket handshake and HS256 JSON Web Tokens with the claims Hasura reads.</summary>
public static class Crypto
{
    /// <summary>The namespace of Hasura's claims inside a token.</summary>
    public const string Claims = "https://hasura.io/jwt/claims";

    private const string UrlSafe = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    private static readonly UTF8Encoding StrictUtf8 = new(false, true);

    public static string Base64(byte[] bytes) => Convert.ToBase64String(bytes);

    public static string Base64Url(byte[] bytes) =>
        Convert.ToBase64String(bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_');

    /// <exception cref="FormatException">The text is not base64url.</exception>
    public static byte[] Base64UrlDecode(string text)
    {
        var output = new List<byte>(text.Length * 3 / 4);
        uint word = 0;
        var bits = 0;
        foreach (var character in text)
        {
            var value = UrlSafe.IndexOf(character);
            if (value < 0)
            {
                throw new FormatException("Invalid base64url text.");
            }
            word = (word << 6) | (uint)value;
            bits += 6;
            if (bits >= 8)
            {
                bits -= 8;
                output.Add((byte)(word >> bits));
                word &= (1u << bits) - 1;
            }
        }
        return output.ToArray();
    }

    /// <summary><c>Sec-WebSocket-Accept</c> of a <c>Sec-WebSocket-Key</c>.</summary>
    public static string WebSocketAccept(string key) =>
        Base64(SHA1.HashData(Encoding.UTF8.GetBytes(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")));

    /// <summary>A token saying its bearer is <paramref name="subject"/>, the way deeplinks signs them.</summary>
    public static string SignToken(byte[] secret, ulong subject, ulong issuedAt)
    {
        var header = Base64Url(Encoding.UTF8.GetBytes("{\"alg\":\"HS256\",\"typ\":\"JWT\"}"));
        var claims = Json.Object(
            (Claims, Json.Object(
                ("x-hasura-allowed-roles", Json.Array([Json.Str("link")])),
                ("x-hasura-default-role", Json.Str("link")),
                ("x-hasura-user-id", Json.Str(subject.ToString(CultureInfo.InvariantCulture))))),
            ("iat", Json.Int(unchecked((long)issuedAt))));
        var signed = header + "." + Base64Url(Encoding.UTF8.GetBytes(claims.Write()));
        return signed + "." + Base64Url(HMACSHA256.HashData(secret, Encoding.UTF8.GetBytes(signed)));
    }

    /// <summary>The subject of a token this secret signed.</summary>
    /// <exception cref="FormatException">The token is expired, forged or malformed.</exception>
    public static ulong VerifyToken(byte[] secret, string token, ulong now)
    {
        var parts = token.Split('.');
        if (parts.Length != 3)
        {
            throw new FormatException("Could not verify JWT: malformed token.");
        }
        var algorithm = Json.Parse(DecodeText(parts[0]));
        if (algorithm.Get("alg")?.AsStr() != "HS256")
        {
            throw new FormatException("Could not verify JWT: only HS256 is accepted.");
        }
        var expected = HMACSHA256.HashData(secret, Encoding.UTF8.GetBytes(parts[0] + "." + parts[1]));
        if (!CryptographicOperations.FixedTimeEquals(Base64UrlDecode(parts[2]), expected))
        {
            throw new FormatException("Could not verify JWT: JWSInvalidSignature");
        }
        var claims = Json.Parse(DecodeText(parts[1]));
        if (claims.Get("exp")?.AsI64() is { } expires && expires <= unchecked((long)now))
        {
            throw new FormatException("Could not verify JWT: JWTExpired");
        }
        var id = claims.Get(Claims)?.Get("x-hasura-user-id")?.AsStr();
        if (id is not null && !id.StartsWith('-')
            && ulong.TryParse(id, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var subject))
        {
            return subject;
        }
        throw new FormatException("Could not verify JWT: no x-hasura-user-id claim.");
    }

    private static string DecodeText(string part)
    {
        try
        {
            return StrictUtf8.GetString(Base64UrlDecode(part));
        }
        catch (DecoderFallbackException error)
        {
            throw new FormatException(error.Message, error);
        }
    }
}
