// What the wire needs of cryptography: SHA-1 for the WebSocket handshake,
// base64 and HS256 JSON Web Tokens with the claims Hasura reads.
// Port of rust/src/graphql/crypto.rs.
//
// The digests come from `node:crypto`, which Bun and Deno serve as well;
// the Rust crate computes SHA-1 and HMAC by hand, with the same bytes out.

import { Buffer } from 'node:buffer';
import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
import { Json, parseU64, utf8Text } from './graphql-json.js';

/** The namespace of Hasura's claims inside a token. */
export const CLAIMS = 'https://hasura.io/jwt/claims';

const URL_SAFE = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';

const encoder = new TextEncoder();
const bytesOf = (data) => (typeof data === 'string' ? encoder.encode(data) : data);

/** Base64 with padding. */
export function base64(bytes) {
  return Buffer.from(bytesOf(bytes)).toString('base64');
}

/** URL-safe base64 without padding. */
export function base64url(bytes) {
  return Buffer.from(bytesOf(bytes)).toString('base64url');
}

/** The bytes of URL-safe base64; trailing bits short of a byte are dropped. */
export function base64urlDecode(text) {
  const out = [];
  let word = 0;
  let bits = 0;
  for (const byte of encoder.encode(text)) {
    const value = URL_SAFE.indexOf(String.fromCharCode(byte));
    if (value < 0) throw new Error('Invalid base64url text.');
    word = (word << 6) | value;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push((word >> bits) & 0xff);
      word &= (1 << bits) - 1;
    }
  }
  return Uint8Array.from(out);
}

export function sha1(message) {
  return new Uint8Array(createHash('sha1').update(bytesOf(message)).digest());
}

/** `Sec-WebSocket-Accept` of a `Sec-WebSocket-Key`. */
export function websocketAccept(key) {
  return base64(sha1(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`));
}

export function hmacSha256(key, message) {
  return new Uint8Array(createHmac('sha256', bytesOf(key)).update(bytesOf(message)).digest());
}

/** A token saying its bearer is `subject`, the way deeplinks signs them. */
export function signToken(secret, subject, issuedAt) {
  const header = base64url('{"alg":"HS256","typ":"JWT"}');
  const claims = Json.object([
    [
      CLAIMS,
      Json.object([
        ['x-hasura-allowed-roles', Json.array([Json.str('link')])],
        ['x-hasura-default-role', Json.str('link')],
        ['x-hasura-user-id', Json.str(BigInt(subject).toString())],
      ]),
    ],
    ['iat', Json.int(BigInt.asIntN(64, BigInt(issuedAt)))],
  ]);
  const signed = `${header}.${base64url(claims.write())}`;
  return `${signed}.${base64url(hmacSha256(secret, signed))}`;
}

/**
 * The subject of a token this secret signed, a BigInt; an expired, forged or
 * malformed token is an error.
 */
export function verifyToken(secret, token, now) {
  const parts = token.split('.');
  if (parts.length !== 3) throw new Error('Could not verify JWT: malformed token.');
  const [header, payload, signature] = parts;
  const algorithm = Json.parse(utf8Text(base64urlDecode(header)));
  if (algorithm.get('alg')?.asStr() !== 'HS256') {
    throw new Error('Could not verify JWT: only HS256 is accepted.');
  }
  const expected = hmacSha256(secret, `${header}.${payload}`);
  const given = base64urlDecode(signature);
  if (given.length !== expected.length || !timingSafeEqual(given, expected)) {
    throw new Error('Could not verify JWT: JWSInvalidSignature');
  }
  const claims = Json.parse(utf8Text(base64urlDecode(payload)));
  const expires = claims.get('exp')?.asI64() ?? null;
  if (expires !== null && expires <= BigInt.asIntN(64, BigInt(now))) {
    throw new Error('Could not verify JWT: JWTExpired');
  }
  const id = claims.get(CLAIMS)?.get('x-hasura-user-id')?.asStr();
  const subject = typeof id === 'string' ? parseU64(id) : null;
  if (subject === null) throw new Error('Could not verify JWT: no x-hasura-user-id claim.');
  return subject;
}
