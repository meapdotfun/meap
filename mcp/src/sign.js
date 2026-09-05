/**
 * Signed requests.
 *
 * The bearer token this replaces had one flaw that no amount of care fixes:
 * the server sees it on every call, so the operator can act as any caller.
 * With play money that is untidy. With anything at stake it means the person
 * running the endpoint can move your money, and no promise not to is worth
 * anything.
 *
 * A signature removes the question. The private key never leaves the caller,
 * the server only ever sees a public key and a signature over the exact bytes
 * it received, and an address is derived from the public key rather than
 * asserted. There is nothing in the server's possession that lets it forge a
 * request, which is what "MEAP does not custody" has to mean before it means
 * anything.
 *
 * Ed25519 through WebCrypto, so the same file runs in Node and in a Worker
 * without a dependency or a polyfill.
 *
 * What is signed is the whole request, not a summary of it:
 *
 *   meap-v1 \n method \n path \n timestamp \n nonce \n body
 *
 * Including the body means arguments cannot be edited in flight. Including the
 * path means a call cannot be replayed against a different route. The
 * timestamp bounds how long a captured request stays useful, and the nonce
 * stops it being used even once more inside that window.
 */

const ALG = { name: 'Ed25519' };
const enc = new TextEncoder();

export const SKEW_MS = 120_000;      // how far a clock may be out
export const SCHEME = 'meap-v1';

// --- bytes ------------------------------------------------------------------

export function toHex(bytes) {
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

export function fromHex(hex) {
  if (typeof hex !== 'string' || hex.length % 2 || /[^0-9a-f]/i.test(hex)) {
    throw new Error('not hex');
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

// --- keys -------------------------------------------------------------------

/** A fresh identity. The private half is the account; nothing recovers it. */
export async function generateKeypair() {
  const pair = await crypto.subtle.generateKey(ALG, true, ['sign', 'verify']);
  return {
    publicKey: toHex(await crypto.subtle.exportKey('raw', pair.publicKey)),
    privateKey: toHex(await crypto.subtle.exportKey('pkcs8', pair.privateKey)),
  };
}

export const importPrivate = (hex) =>
  crypto.subtle.importKey('pkcs8', fromHex(hex), ALG, false, ['sign']);

export const importPublic = (hex) =>
  crypto.subtle.importKey('raw', fromHex(hex), ALG, false, ['verify']);

// --- the payload ------------------------------------------------------------

/**
 * Exactly what gets signed. Both sides build this the same way from the same
 * pieces, so a mismatch anywhere in the request is a failed verification
 * rather than a subtle difference in interpretation.
 */
export function payload({ method, path, time, nonce, body }) {
  return enc.encode([SCHEME, method.toUpperCase(), path, String(time), nonce, body ?? ''].join('\n'));
}

/** Headers for one signed request. */
export async function signRequest(privateKeyHex, { method, path, body }) {
  const key = await importPrivate(privateKeyHex);
  const time = Date.now();
  const nonce = toHex(crypto.getRandomValues(new Uint8Array(16)));
  const sig = await crypto.subtle.sign(ALG, key, payload({ method, path, time, nonce, body }));
  return {
    'x-meap-time': String(time),
    'x-meap-nonce': nonce,
    'x-meap-sig': toHex(sig),
  };
}

/**
 * Check a signed request.
 *
 * `seen` is how replay is stopped: a nonce that has been used before is
 * refused even inside the time window. The caller owns that store, because
 * only it knows how long to keep them, and the answer is the width of the skew
 * window and no longer.
 *
 * Returns the public key on success. Throws with a reason otherwise, since a
 * caller getting this wrong needs to know which part.
 */
export async function verifyRequest({ publicKey, time, nonce, signature, method, path, body, now, seen }) {
  if (!publicKey || !time || !nonce || !signature) throw new Error('a signed request needs a key, a time, a nonce and a signature');

  const t = Number(time);
  if (!Number.isFinite(t)) throw new Error('x-meap-time is not a number');
  const drift = Math.abs((now ?? Date.now()) - t);
  if (drift > SKEW_MS) throw new Error(`x-meap-time is ${Math.round(drift / 1000)}s out; the window is ${SKEW_MS / 1000}s`);

  if (seen && await seen.has(nonce)) throw new Error('that nonce has been used; every request needs a new one');

  let key;
  try { key = await importPublic(publicKey); } catch { throw new Error('x-meap-key is not a 32 byte ed25519 public key in hex'); }

  let ok = false;
  try {
    ok = await crypto.subtle.verify(ALG, key, fromHex(signature), payload({ method, path, time: t, nonce, body }));
  } catch { throw new Error('x-meap-sig is not a signature'); }
  if (!ok) throw new Error('signature does not match this request');

  if (seen) await seen.add(nonce, t);
  return publicKey;
}
