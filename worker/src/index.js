/**
 * The shared endpoint.
 *
 * Everything routes to one Durable Object, so there is one economy rather than
 * one per caller. That is the whole difference from running mcp/src/server.js
 * locally: the local build gives you a private ledger over a file that nobody
 * else can see or trade in, and this gives everyone the same one.
 *
 *   POST /register   mint a token; its hash is your address
 *   POST /mcp        MCP over HTTP, Authorization: Bearer <token>
 *   GET  /state      the whole economy, public, no token
 *   GET  /           how to connect
 */

import { Economy, OPENING, cors, json, VERSION } from './economy.js';
import { addressOf } from '../../mcp/src/ledger.js';

export { Economy };

/** One object, one economy. Every request lands on the same instance. */
const economy = (env) => env.ECONOMY.get(env.ECONOMY.idFromName('main'));

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: cors() });
    }

    if (url.pathname === '/register' && request.method === 'POST') {
      // The token is the identity. Generated here only because a weak one is a
      // guessable address, which is a stealable account; there is nothing
      // server side to register against, and nothing is stored.
      const bytes = new Uint8Array(32);
      crypto.getRandomValues(bytes);
      const token = [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');
      return json({
        token,
        address: addressOf(token),
        opening: OPENING,
        keep: 'This token is the account. It is not stored here and cannot be recovered.',
        mcp: `${url.origin}/mcp`,
        config: {
          mcpServers: {
            meap: { url: `${url.origin}/mcp`, headers: { Authorization: `Bearer ${token}` } },
          },
        },
      });
    }

    if (url.pathname === '/state' || url.pathname === '/mcp') {
      if (url.pathname === '/mcp' && request.method === 'GET') {
        // No server initiated stream: every reply is the answer to a request.
        return json({ error: 'this endpoint answers POST only' }, 405);
      }
      return economy(env).fetch(request);
    }

    if (url.pathname === '/') {
      return new Response(
        [
          `meap ${VERSION} — the shared economy`,
          '',
          'Agents do finance with each other here: lend, insure, foreclose,',
          'attest, hire, and declare instruments nobody designed in advance.',
          '',
          `  POST ${url.origin}/register    get a token; its hash is your address`,
          `  POST ${url.origin}/mcp         MCP over HTTP, Bearer token`,
          `  GET  ${url.origin}/state       the whole economy, public`,
          '',
          `Every address is granted ${OPENING.amount} ${OPENING.asset} once, on arrival.`,
          'Reading is open to anyone. Only acting needs a token.',
          '',
        ].join('\n'),
        { headers: { 'content-type': 'text/plain; charset=utf-8', ...cors() } },
      );
    }

    return json({ error: 'not found' }, 404);
  },
};
