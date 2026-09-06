/**
 * A signing MCP-over-HTTP agent, for scripts.
 *
 * The endpoint takes signatures and nothing else, so a script that used to
 * POST a bearer token now holds a keypair and signs each call. There is no
 * registration: the public key is the address, joining happens on first
 * contact, and the opening grant lands then. This is the same thing
 * mcp/src/client.js does for a live MCP client, minus the stdio plumbing.
 */
import { generateKeypair, signRequest } from '../../mcp/src/sign.js';
import { addressOfKey } from '../../mcp/src/ledger.js';

/** A fresh signing identity bound to one endpoint. */
export async function newAgent(base) {
  const { publicKey, privateKey } = await generateKeypair();
  const address = addressOfKey(publicKey);
  const path = '/';
  let seq = 0;

  async function rpc(method, params) {
    const body = JSON.stringify({ jsonrpc: '2.0', id: ++seq, method, params });
    const sig = await signRequest(privateKey, { method: 'POST', path, body });
    const r = await fetch(base.replace(/\/+$/, '') + path, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-meap-key': publicKey, ...sig },
      body,
    });
    if (r.status === 202) return null;
    return r.json();
  }

  /** Call a tool; return { isError, text, json }. */
  async function call(name, args) {
    const r = await rpc('tools/call', { name, arguments: args ?? {} });
    const text = r?.result?.content?.[0]?.text ?? JSON.stringify(r);
    let json = null;
    try { json = JSON.parse(text); } catch { /* a refusal is prose */ }
    return { isError: !!r?.result?.isError, text, json };
  }

  return { address, publicKey, rpc, call };
}
