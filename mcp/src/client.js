#!/usr/bin/env node
/**
 * A signing proxy: MCP over stdio on this side, signed HTTP on the other.
 *
 * This exists because of a gap. Signatures are the only honest way to prove
 * who is calling, but an MCP client configured with a URL can only attach
 * fixed headers; it cannot compute a signature per request. So the key lives
 * here, in a process on your own machine, and this forwards.
 *
 * The shape is the same one a hardware wallet uses. Your client talks to
 * something local that holds the key, and the key never crosses the network.
 * The endpoint sees a public key and a signature over exactly the bytes it
 * received, and holds nothing that could forge a request in your name.
 *
 *   {
 *     "mcpServers": {
 *       "meap": {
 *         "command": "node",
 *         "args": ["/path/to/meap/mcp/src/client.js"]
 *       }
 *     }
 *   }
 *
 *   MEAP_REMOTE    endpoint to forward to. Default https://mcp.meap.fun
 *   MEAP_KEY_FILE  where the private key lives. Default ./meap.key
 *   MEAP_KEY       the key itself, if you would rather not use a file
 *
 * On first run it generates a key, writes it, and prints the address it
 * derives. That file is the account. Nothing here or at the far end can
 * recover it, because nowhere else has ever held it.
 */

import { createInterface } from 'node:readline';
import { readFileSync, writeFileSync, existsSync, chmodSync } from 'node:fs';

import { generateKeypair, signRequest, importPrivate, toHex } from './sign.js';
import { addressOfKey } from './ledger.js';

const REMOTE = (process.env.MEAP_REMOTE || 'https://mcp.meap.fun').replace(/\/+$/, '');
const KEY_FILE = process.env.MEAP_KEY_FILE || 'meap.key';

const note = (s) => process.stderr.write(s + '\n');

// --- the key ----------------------------------------------------------------

async function loadKey() {
  if (process.env.MEAP_KEY) return process.env.MEAP_KEY.trim();

  if (existsSync(KEY_FILE)) {
    const hex = readFileSync(KEY_FILE, 'utf8').trim();
    try {
      await importPrivate(hex);                 // fail now, not on the first call
      return hex;
    } catch {
      note(`${KEY_FILE} does not contain a usable key. Move it aside to start over.`);
      process.exit(2);
    }
  }

  const pair = await generateKeypair();
  writeFileSync(KEY_FILE, pair.privateKey + '\n', { mode: 0o600 });
  try { chmodSync(KEY_FILE, 0o600); } catch { /* windows */ }
  note(`generated a key in ${KEY_FILE}. That file is the account; nothing can recover it.`);
  return pair.privateKey;
}

const privateKey = await loadKey();
const publicKey = toHex(await import('node:crypto').then(async () => {
  // Derive the public half from the private one by round tripping through a
  // signature-capable import, which is the only way WebCrypto exposes it.
  const { createPrivateKey, createPublicKey } = await import('node:crypto');
  const der = Buffer.from(privateKey, 'hex');
  const pub = createPublicKey(createPrivateKey({ key: der, format: 'der', type: 'pkcs8' }));
  const raw = pub.export({ format: 'der', type: 'spki' });
  return raw.subarray(raw.length - 32);         // spki suffix is the raw key
}));

const ME = addressOfKey(publicKey);

// --- forward ----------------------------------------------------------------

async function forward(message) {
  const body = JSON.stringify(message);
  const path = '/';
  const headers = {
    'content-type': 'application/json',
    'x-meap-key': publicKey,
    ...(await signRequest(privateKey, { method: 'POST', path, body })),
  };

  const r = await fetch(REMOTE + path, { method: 'POST', headers, body });
  if (r.status === 202) return null;            // notifications get no reply
  const text = await r.text();
  try {
    return JSON.parse(text);
  } catch {
    return { jsonrpc: '2.0', id: message.id ?? null, error: { code: -32603, message: `${r.status} from ${REMOTE}: ${text.slice(0, 200)}` } };
  }
}

const rl = createInterface({ input: process.stdin });
rl.on('line', async (line) => {
  if (!line.trim()) return;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } }) + '\n');
    return;
  }
  try {
    const reply = await forward(msg);
    if (reply) process.stdout.write(JSON.stringify(reply) + '\n');
  } catch (e) {
    // A network failure is the proxy's problem, not the protocol's, so it
    // comes back as an error on the request rather than killing the session.
    if (msg.id !== undefined) {
      process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: `cannot reach ${REMOTE}: ${e.message}` } }) + '\n');
    }
  }
});

note(`meap signing proxy -> ${REMOTE}`);
note(`acting as ${ME}`);
