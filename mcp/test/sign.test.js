/**
 * What a signature has to withstand.
 *
 * The scheme replaces a bearer token, whose flaw was that the server saw it
 * and could therefore act as any caller. These tests are the claim that the
 * replacement does not have a version of the same problem: nothing the server
 * receives can be turned into a different valid request.
 *
 * Run: npm test
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';

import { generateKeypair, signRequest, verifyRequest, payload, SKEW_MS } from '../src/sign.js';
import { addressOfKey, addressOf } from '../src/ledger.js';

const BODY = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'pay', arguments: { amount: 10 } } });

/** A nonce store, as the worker keeps one. */
const store = () => {
  const seen = new Set();
  return { has: async (n) => seen.has(n), add: async (n) => { seen.add(n); } };
};

async function signed(over = {}) {
  const pair = await generateKeypair();
  const req = { method: 'POST', path: '/', body: BODY, ...over };
  const h = await signRequest(pair.privateKey, req);
  return {
    pair,
    args: {
      publicKey: pair.publicKey,
      time: h['x-meap-time'], nonce: h['x-meap-nonce'], signature: h['x-meap-sig'],
      method: req.method, path: req.path, body: req.body,
      now: Date.now(), seen: store(),
    },
  };
}

test('a signature over the request it was made for is accepted', async () => {
  const { args, pair } = await signed();
  assert.equal(await verifyRequest(args), pair.publicKey);
});

test('a captured request cannot be sent twice', async () => {
  // The whole point of the nonce. Without it, anyone who observed one valid
  // request could repeat it for as long as the clock window allowed, which for
  // `pay` means sending the same money again.
  const { args } = await signed();
  await verifyRequest(args);
  await assert.rejects(() => verifyRequest(args), /nonce has been used/);
});

test('the arguments cannot be edited in flight', async () => {
  const { args } = await signed();
  const tampered = { ...args, body: BODY.replace('"amount":10', '"amount":1000000') };
  await assert.rejects(() => verifyRequest(tampered), /does not match/);
});

test('a call cannot be replayed against a different route', async () => {
  const { args } = await signed();
  await assert.rejects(() => verifyRequest({ ...args, path: '/register' }), /does not match/);
});

test('a request cannot be held and used later', async () => {
  const { args } = await signed();
  await assert.rejects(
    () => verifyRequest({ ...args, now: Date.now() + SKEW_MS + 60_000 }),
    /out; the window is/,
  );
});

test('signing with one key and claiming another fails', async () => {
  const { args } = await signed();
  const other = await generateKeypair();
  await assert.rejects(() => verifyRequest({ ...args, publicKey: other.publicKey }), /does not match/);
});

test('a malformed key or signature is refused by name', async () => {
  const { args } = await signed();
  await assert.rejects(() => verifyRequest({ ...args, publicKey: 'nonsense' }), /not a 32 byte ed25519/);
  await assert.rejects(() => verifyRequest({ ...args, signature: 'zz' }), /not a signature/);
  await assert.rejects(() => verifyRequest({ ...args, time: 'soon' }), /not a number/);
  await assert.rejects(() => verifyRequest({ ...args, signature: undefined }), /needs a key, a time/);
});

test('the signed payload covers every part that matters', async () => {
  const base = { method: 'POST', path: '/', time: 1, nonce: 'n', body: 'b' };
  const of = (o) => new TextDecoder().decode(payload({ ...base, ...o }));
  const original = of({});
  for (const change of [{ method: 'GET' }, { path: '/x' }, { time: 2 }, { nonce: 'm' }, { body: 'c' }]) {
    assert.notEqual(of(change), original, `changing ${Object.keys(change)[0]} must change what is signed`);
  }
});

test('a key address and a label address can never collide', async () => {
  // They are different strengths. If a label could land on a key's address,
  // the weaker scheme would be able to spend the stronger one's balance.
  const pair = await generateKeypair();
  assert.notEqual(addressOfKey(pair.publicKey), addressOf(pair.publicKey));
  assert.match(addressOfKey(pair.publicKey), /^ag_[0-9a-f]{32}$/);
  assert.equal(addressOfKey(pair.publicKey), addressOfKey(pair.publicKey.toUpperCase()),
    'case in the key must not change the address');
});
