// The cryptography of the wire: the tests of rust/src/graphql/crypto.rs, the
// WebSocket handshake of RFC 6455 and the tokens deeplinks signs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';

import {
  CLAIMS,
  base64,
  base64url,
  base64urlDecode,
  hmacSha256,
  sha1,
  signToken,
  verifyToken,
  websocketAccept,
} from '../src/graphql-crypto.js';

const hex = (bytes) => Buffer.from(bytes).toString('hex');

function tokenWith(secret, header, claims) {
  const signed = `${base64url(header)}.${base64url(claims)}`;
  return `${signed}.${base64url(hmacSha256(secret, signed))}`;
}

describe('tokens', () => {
  it('are refused for their algorithm, expiry and subject', () => {
    const hs256 = '{"alg":"HS256"}';
    const message = (token) => {
      try {
        verifyToken('k', token, 100);
      } catch (error) {
        return error.message;
      }
      return null;
    };
    assert.equal(
      message(tokenWith('k', '{"alg":"none"}', '{}')),
      'Could not verify JWT: only HS256 is accepted.',
    );
    assert.equal(
      message(tokenWith('k', '{}', '{}')),
      'Could not verify JWT: only HS256 is accepted.',
    );
    assert.equal(message(tokenWith('k', hs256, '{"exp":100}')), 'Could not verify JWT: JWTExpired');
    const nobody = 'Could not verify JWT: no x-hasura-user-id claim.';
    assert.equal(message(tokenWith('k', hs256, '{"exp":101}')), nobody);
    assert.equal(
      message(tokenWith('k', hs256, `{"${CLAIMS}":{"x-hasura-user-id":"not a number"}}`)),
      nobody,
    );
    assert.equal(message(tokenWith('k', hs256, `{"${CLAIMS}":{"x-hasura-user-id":7}}`)), nobody);
    assert.equal(
      message(tokenWith('other', hs256, '{}')),
      'Could not verify JWT: JWSInvalidSignature',
    );
    const [head, body] = tokenWith('k', hs256, '{}').split('.');
    assert.equal(message(`${head}.${body}.AAAA`), 'Could not verify JWT: JWSInvalidSignature');
    assert.equal(message('a.b'), 'Could not verify JWT: malformed token.');
    assert.equal(message('a.b.c.d'), 'Could not verify JWT: malformed token.');
    assert.equal(message('!.b.c'), 'Invalid base64url text.');
    assert.equal(
      message(`${base64url([0xff])}.b.c`),
      'invalid utf-8 sequence of 1 bytes from index 0',
    );
  });

  it('verify until they are forged', () => {
    const token = signToken('secret', 42n, 1n);
    assert.equal(verifyToken(Buffer.from('secret'), token, 2), 42n);
    assert.throws(() => verifyToken('other', token, 2));
    assert.throws(() => verifyToken('secret', 'a.b', 2));
    const expiring = tokenWith(
      'k',
      '{"alg":"HS256"}',
      `{"exp":5,"${CLAIMS}":{"x-hasura-user-id":"9"}}`,
    );
    assert.equal(verifyToken('k', expiring, 4), 9n);
  });

  it('are what deeplinks signs', () => {
    // Recorded from a deeplinks server with its public default key.
    const secret = '3EK6FD+o0+c7tzBNVfjpMkNDi2yARAAKzQlk8O2IKoxQu4nF7EdAh8s3TwpHwrdWT6R';
    const token = signToken(secret, 7, 1);
    assert.ok(token.startsWith('eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.'));
    assert.equal(verifyToken(secret, token, 2), 7n);
    const claims = Buffer.from(token.split('.')[1], 'base64url').toString('utf8');
    assert.equal(
      claims,
      `{"${CLAIMS}":{"x-hasura-allowed-roles":["link"],"x-hasura-default-role":"link","x-hasura-user-id":"7"},"iat":1}`,
    );
  });
});

describe('digests and base64', () => {
  it('match RFC 6455 and the published vectors', () => {
    assert.equal(websocketAccept('dGhlIHNhbXBsZSBub25jZQ=='), 's3pPLMBiTxaQ9kYGzzhZRbK+xOo=');
    assert.equal(hex(sha1('abc')), 'a9993e364706816aba3e25717850c26c9cd0d89d');
    assert.equal(
      hex(hmacSha256('key', 'The quick brown fox jumps over the lazy dog')),
      'f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8',
    );
  });

  it('encode and decode', () => {
    assert.equal(base64('ab'), 'YWI=');
    assert.equal(base64url([0xfb, 0xff]), '-_8');
    assert.deepEqual([...base64urlDecode('-_8')], [0xfb, 0xff]);
    // Bits short of a byte are dropped.
    assert.deepEqual([...base64urlDecode('a')], []);
    assert.deepEqual([...base64urlDecode('YWI')], [0x61, 0x62]);
    assert.throws(() => base64urlDecode('YW='), { message: 'Invalid base64url text.' });
  });
});
