//! What the wire needs of cryptography: SHA-1 for the WebSocket handshake,
//! base64 and HS256 JSON Web Tokens with the claims Hasura reads.

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};

use super::json::Json;

/// The namespace of Hasura's claims inside a token.
pub const CLAIMS: &str = "https://hasura.io/jwt/claims";

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode(bytes: &[u8], alphabet: &[u8; 64], padding: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for position in 0..4 {
            if position <= chunk.len() {
                out.push(alphabet[(word >> (18 - position * 6)) as usize & 63] as char);
            } else if padding {
                out.push('=');
            }
        }
    }
    out
}

pub fn base64(bytes: &[u8]) -> String {
    encode(bytes, STANDARD, true)
}

pub fn base64url(bytes: &[u8]) -> String {
    encode(bytes, URL_SAFE, false)
}

pub fn base64url_decode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut word = 0u32;
    let mut bits = 0;
    for byte in text.bytes() {
        let value = URL_SAFE
            .iter()
            .position(|&letter| letter == byte)
            .ok_or_else(|| anyhow!("Invalid base64url text."))? as u32;
        word = word << 6 | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((word >> bits) as u8);
            word &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

pub fn sha1(message: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&((message.len() as u64) * 8).to_be_bytes());
    for block in padded.chunks(64) {
        let mut words = [0u32; 80];
        for (word, bytes) in words.iter_mut().zip(block.chunks(4)) {
            *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for at in 16..80 {
            words[at] =
                (words[at - 3] ^ words[at - 8] ^ words[at - 14] ^ words[at - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (at, word) in words.iter().enumerate() {
            let (f, k) = match at {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (value, add) in state.iter_mut().zip([a, b, c, d, e]) {
            *value = value.wrapping_add(add);
        }
    }
    let mut out = [0; 20];
    for (bytes, word) in out.chunks_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// `Sec-WebSocket-Accept` of a `Sec-WebSocket-Key`.
pub fn websocket_accept(key: &str) -> String {
    base64(&sha1(
        format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes(),
    ))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.iter().map(|value| value ^ byte).collect::<Vec<u8>>();
    let mut inner = Sha256::new();
    inner.update(pad(0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(pad(0x5c));
    outer.update(inner.finalize());
    outer.finalize().to_vec()
}

/// A token saying its bearer is `subject`, the way deeplinks signs them.
pub fn sign_token(secret: &[u8], subject: u64, issued_at: u64) -> String {
    let header = base64url(br#"{"alg":"HS256","typ":"JWT"}"#);
    let claims = Json::object(vec![
        (
            CLAIMS,
            Json::object(vec![
                (
                    "x-hasura-allowed-roles",
                    Json::Array(vec![Json::str("link")]),
                ),
                ("x-hasura-default-role", Json::str("link")),
                ("x-hasura-user-id", Json::str(subject.to_string())),
            ]),
        ),
        ("iat", Json::Int(issued_at as i64)),
    ]);
    let payload = base64url(claims.write().as_bytes());
    let signed = format!("{header}.{payload}");
    let signature = base64url(&hmac_sha256(secret, signed.as_bytes()));
    format!("{signed}.{signature}")
}

/// The subject of a token this secret signed; an expired, forged or malformed
/// token is an error.
pub fn verify_token(secret: &[u8], token: &str, now: u64) -> Result<u64> {
    let parts: Vec<&str> = token.split('.').collect();
    let [header, payload, signature] = parts.as_slice() else {
        bail!("Could not verify JWT: malformed token.");
    };
    let algorithm = Json::parse(&String::from_utf8(base64url_decode(header)?)?)?;
    if algorithm.get("alg").and_then(Json::as_str) != Some("HS256") {
        bail!("Could not verify JWT: only HS256 is accepted.");
    }
    let expected = hmac_sha256(secret, format!("{header}.{payload}").as_bytes());
    let given = base64url_decode(signature)?;
    let same = given.len() == expected.len()
        && given
            .iter()
            .zip(&expected)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0;
    if !same {
        bail!("Could not verify JWT: JWSInvalidSignature");
    }
    let claims = Json::parse(&String::from_utf8(base64url_decode(payload)?)?)?;
    if let Some(expires) = claims.get("exp").and_then(Json::as_i64) {
        if expires <= now as i64 {
            bail!("Could not verify JWT: JWTExpired");
        }
    }
    claims
        .get(CLAIMS)
        .and_then(|hasura| hasura.get("x-hasura-user-id"))
        .and_then(Json::as_str)
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| anyhow!("Could not verify JWT: no x-hasura-user-id claim."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_and_the_handshake_match_rfc_6455() {
        assert_eq!(
            websocket_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn a_token_verifies_until_it_is_forged() {
        let token = sign_token(b"secret", 42, 1);
        assert_eq!(verify_token(b"secret", &token, 2).unwrap(), 42);
        assert!(verify_token(b"other", &token, 2).is_err());
        assert!(verify_token(b"secret", "a.b", 2).is_err());
    }

    #[test]
    fn verifies_a_token_deeplinks_signed() {
        // Recorded from a deeplinks server with its public default key.
        let secret = b"3EK6FD+o0+c7tzBNVfjpMkNDi2yARAAKzQlk8O2IKoxQu4nF7EdAh8s3TwpHwrdWT6R";
        let token = sign_token(secret, 7, 1);
        assert!(token.starts_with("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9."));
        assert_eq!(verify_token(secret, &token, 2).unwrap(), 7);
    }
}
