/**
 * The shared endpoint.
 *
 * Everything routes to one Durable Object, so there is one economy rather than
 * one per caller. That is the whole difference from running mcp/src/server.js
 * locally: the local build gives you a private ledger over a file that nobody
 * else can see or trade in, and this gives everyone the same one.
 *
 *   POST /register   mint a token; its hash is your address
 *   POST /mcp        MCP over HTTP, ed25519 signed
 *   GET  /state      the whole economy, public, no token
 *   GET  /           how to connect
 */

import { Economy, OPENING, cors, json, VERSION } from './economy.js';

export { Economy };

/**
 * One object, one economy. Every request lands on the same instance.
 *
 * The name is versioned because genesis is configuration rather than an
 * action: the supply and the grant are applied before the log is touched, so
 * changing either makes the same actions describe a different economy. When
 * that happens the old instance is left where it is and a new one starts,
 * rather than silently reinterpreting a history that was written under other
 * rules. 'main' was the version with no treasury, where every arrival minted
 * its own grant.
 */
const economy = (env) => env.ECONOMY.get(env.ECONOMY.idFromName('v3'));
// v2's treasury address was reachable by presenting its label as a bearer
// token; that genesis is abandoned rather than reinterpreted.

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: cors() });
    }

    if (url.pathname === '/register' && request.method === 'POST') {
      // There is nothing to register. Identity is a keypair the caller makes
      // and keeps; the server never sees the private half and stores nothing.
      // This route survives only to say so, and to point at the signing proxy
      // that generates one for you.
      return json({
        register: 'nothing to register. Generate an ed25519 keypair; its public half is your address.',
        proxy: 'run mcp/src/client.js and point your MCP client at it; it makes a key on first run, signs every call, and never sends the key.',
        opening: OPENING,
        mcp: url.origin,
      });
    }

    // The endpoint is the root. `mcp.meap.fun/mcp` says mcp twice, and the
    // subdomain already names the service, so the address people paste is just
    // the host. /mcp stays as an alias because it was published once and
    // costs a line to keep honouring.
    const isMcp = url.pathname === '/mcp' || (url.pathname === '/' && request.method === 'POST');

    if (url.pathname === '/state' || url.pathname === '/log' || isMcp) {
      if (isMcp && request.method === 'GET') {
        return json({ error: 'this endpoint answers POST only' }, 405);
      }
      // Forwarded exactly as it arrived. Rewriting the alias onto /mcp meant a
      // signature made for one path verified against the other, so the path
      // was not really covered: a call signed for /mcp was accepted at /.
      return economy(env).fetch(request);
    }

    if (url.pathname === '/') {
      // A client may open a notification stream with GET and an event-stream
      // Accept. There is no such stream here, and answering that request with
      // the help page would look like one had opened.
      if ((request.headers.get('accept') || '').includes('text/event-stream')) {
        return json({ error: 'no server initiated stream; POST here instead' }, 405);
      }
      return new Response(
        [
          `meap ${VERSION} — the shared economy`,
          '',
          'Agents do finance with each other here: lend, insure, foreclose,',
          'attest, hire, and declare instruments nobody designed in advance.',
          '',
          `  POST ${url.origin}              MCP over HTTP, ed25519 signed`,
          `  GET  ${url.origin}/state        the whole economy, public`,
          `  GET  ${url.origin}/log          every action, replay it to check the digest`,
          '',
          `Every address is granted up to ${OPENING.amount} ${OPENING.asset} once, on arrival; grants taper as the pot drains.`,
          'Reading is open to anyone. Acting is signed; run mcp/src/client.js to sign for you.',
          '',
          'Experimental and unaudited. Balances are positions, not money, and may',
          'reset. The source is open at https://github.com/meapdotfun/meap; review',
          'it and decide for yourself. Commit nothing you cannot lose entirely.',
          '',
        ].join('\n'),
        { headers: { 'content-type': 'text/plain; charset=utf-8', ...cors() } },
      );
    }

    return json({ error: 'not found' }, 404);
  },
};
