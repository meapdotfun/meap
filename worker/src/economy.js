/**
 * The shared economy, as a Durable Object.
 *
 * A Durable Object is an unusually good fit for this ledger and the reason is
 * structural rather than convenient. The ledger is a single threaded state
 * machine over an append only log: `apply` either accepts an action and mutates,
 * or throws and changes nothing. Cloudflare gives exactly one instance of a
 * named object, running one request at a time, with storage attached. So the
 * serialisation the ledger already assumed is provided by the platform instead
 * of by a lock, and there is no database to keep consistent with it.
 *
 * Nothing in mcp/src is modified to run here. The ledger, the grammar, the
 * settlement engine and the verb set import unchanged, because none of them
 * ever touched a clock, a random number, or a Node API. That was worth the
 * discipline: the same code decides the rules locally and in production.
 *
 * Identity is the bearer token, hashed. `addressOf(token)` derives the address,
 * so the server stores no secret and holds no key: it only ever sees what the
 * caller presents. Losing the token loses the account, exactly as losing a
 * private key would. This is not signing, and the honest limit is that the
 * server sees the token on every call and could act as you; a real deployment
 * wants per request signatures, which stock MCP clients cannot yet produce.
 */

import { Ledger, addressOf, addressOfKey } from '../../mcp/src/ledger.js';
import { verifyRequest, SKEW_MS } from '../../mcp/src/sign.js';
import { makeTools, dispatch, READS, VERSION } from '../../mcp/src/tools.js';

/** Credited once, when an address first appears. Equal for everyone. */
export const OPENING = { asset: 'USD', amount: 1_000_000 };

/**
 * The entire supply, held at genesis and paid out as arrivals join.
 *
 * Registration used to mint its own grant, which made it a money press: three
 * registrations against the live endpoint added three million out of nothing.
 * A fixed pot turns that into a drain on something finite, and makes the total
 * on the ledger a number that means something.
 */
export const TREASURY = { address: addressOf('meap:treasury:v1'), asset: 'USD', amount: 500_000_000 };

const KEY = (n) => `a:${String(n).padStart(12, '0')}`;

export class Economy {
  constructor(state, env) {
    this.state = state;
    this.env = env;
    this.ledger = null;
    this.n = 0;
    // Actions applied by the current message, awaiting their durable write.
    this.pending = [];
  }

  /**
   * Replay the log into memory.
   *
   * Only ever runs on a cold start, since the object stays resident between
   * requests. `blockConcurrencyWhile` holds every other request until it is
   * done, so no caller can observe a half replayed economy.
   */
  async boot() {
    if (this.ledger) return;
    await this.state.blockConcurrencyWhile(async () => {
      if (this.ledger) return;
      const l = new Ledger({ playground: false, opening: OPENING, treasury: TREASURY });
      let n = 0;
      let after;
      for (;;) {
        const page = await this.state.storage.list({ prefix: 'a:', limit: 1000, startAfter: after });
        if (page.size === 0) break;
        for (const [k, action] of page) { l.apply(action); after = k; n++; }
        if (page.size < 1000) break;
      }
      this.ledger = l;
      this.n = n;
    });
  }

  /**
   * Apply then persist.
   *
   * `apply` validates and throws before anything is written, so a refused
   * action never reaches storage. If the write itself fails the in memory
   * ledger is ahead of the log, which would make the digest a lie, so the
   * ledger is dropped and the next request rebuilds from what was actually
   * durable.
   */
  async commit(action) {
    const result = this.ledger.apply(action);
    try {
      await this.state.storage.put(KEY(this.n), action);
      this.n += 1;
    } catch (e) {
      this.ledger = null;
      throw new Error(`could not record that action, nothing was applied: ${e.message}`);
    }
    return result;
  }

  /**
   * Nonces already spent, so a captured request cannot be sent twice.
   *
   * Only kept for the width of the clock skew window, because outside it the
   * timestamp check refuses the request anyway and the nonce is dead weight.
   */
  nonces() {
    const store = this.state.storage;
    return {
      has: async (n) => (await store.get(`n:${n}`)) !== undefined,
      add: async (n, t) => {
        await store.put(`n:${n}`, t);
        // Opportunistic sweep; there is no cron here and this is cheap.
        if (Math.abs(t) % 32 === 0) {
          const old = await store.list({ prefix: 'n:', limit: 200 });
          const cutoff = Date.now() - SKEW_MS * 2;
          const dead = [...old].filter(([, when]) => when < cutoff).map(([k]) => k);
          if (dead.length) await store.delete(dead);
        }
      },
    };
  }

  /** Join on first contact, which is also when the opening grant lands. */
  async ensure(address) {
    if (!this.ledger.agents.has(address)) {
      await this.commit({ type: 'join', by: address, at: Date.now() });
    }
  }

  async fetch(request) {
    await this.boot();
    const url = new URL(request.url);

    if (url.pathname === '/state') return this.publicState();
    if (url.pathname === '/log') return this.exportLog();
    // Both spellings, and each verifies against the path the caller actually
    // signed, rather than one being rewritten into the other.
    if (url.pathname === '/mcp' || (url.pathname === '/' && request.method === 'POST')) {
      return this.mcp(request);
    }
    return json({ error: 'not found' }, 404);
  }

  /**
   * Everything a spectator can see, which is everything.
   *
   * Reading was never meant to be gated: an economy whose participants are
   * programs is worth watching, and the digest lets anyone recompute it rather
   * than trust this response.
   */
  publicState() {
    const l = this.ledger;
    return json({
      digest: l.digest(),
      actions: l.log.length,
      totals: Object.fromEntries(l.audit()),
      opening: OPENING,
      // The supply, so a reader can check the totals without fetching the log.
      // It never moves: grants are paid out of it, not printed alongside it.
      supply: { asset: TREASURY.asset, amount: TREASURY.amount, held: l.balance(TREASURY.address, TREASURY.asset) },
      agents: [...l.agents.values()].map((a) => ({
        address: a.address,
        balances: Object.fromEntries(a.balances),
        stats: a.stats,
      })),
      markets: [...l.markets.values()].map((m) => ({
        id: m.id, declarer: m.declarer, state: m.state, escrow: m.escrow,
        legs: m.declaration.positions.legs, label: m.declaration.label,
        mechanism: m.declaration.mechanism.kind, expiry: m.declaration.expiry,
        holders: m.holdings.size,
      })),
      offers: [...l.offers.values()].filter((o) => o.state === 'open').length,
    });
  }

  /**
   * The whole log, plus the genesis it was replayed against.
   *
   * This is the backup. State here is whatever replaying the log produces, so
   * a copy of the log is a complete copy of the economy, and anyone can hold
   * one. It is also the audit: replay it, compare the digest against /state,
   * and the ledger has proved itself rather than asked to be believed.
   *
   * Genesis travels with it because the treasury is configuration rather than
   * an action. A log replayed against a different supply is a different and
   * equally self consistent economy.
   */
  exportLog() {
    return json({
      genesis: { opening: OPENING, treasury: TREASURY },
      digest: this.ledger.digest(),
      actions: this.ledger.log,
    });
  }

  async mcp(request) {
    // The body is read once, as text, because a signature covers the exact
    // bytes that arrived. Re-serialising the parsed object would change
    // whitespace and key order and verify against nothing.
    const raw = await request.text();

    // Two ways to be somebody, and they are not equal.
    //
    // A signature is the real one: the private key never leaves the caller,
    // this server sees only a public key and a signature over what it was
    // sent, and it holds nothing that could forge a request in your name.
    //
    // A bearer token still works because it is what the first version shipped
    // with and it is fine for a playground. It is strictly weaker, and the
    // difference is not subtle: whoever sees the token can act as you, and
    // this server sees it on every call.
    let me = null;
    let how = null;
    const key = request.headers.get('x-meap-key');
    if (key) {
      try {
        await verifyRequest({
          publicKey: key,
          time: request.headers.get('x-meap-time'),
          nonce: request.headers.get('x-meap-nonce'),
          signature: request.headers.get('x-meap-sig'),
          method: request.method,
          path: new URL(request.url).pathname,
          body: raw,
          now: Date.now(),
          seen: this.nonces(),
        });
      } catch (e) {
        return json({ jsonrpc: '2.0', id: null, error: { code: -32002, message: `signature refused: ${e.message}` } }, 401);
      }
      me = addressOfKey(key);
      how = 'signature';
    } else {
      const token = bearer(request);
      if (token) { me = addressOf(token); how = 'bearer'; }
    }
    if (me) await this.ensure(me);

    let body;
    try {
      body = JSON.parse(raw);
    } catch {
      return json({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } }, 400);
    }

    const tools = makeTools({
      ledger: this.ledger,
      me,
      // The tool layer is synchronous by design, so an action is queued here
      // and awaited after dispatch returns. A Durable Object runs one request
      // at a time, so nothing can interleave in between.
      commit: (action) => {
        if (!me) throw new Error(NEEDS_TOKEN);
        const result = this.ledger.apply(action);
        this.pending.push(action);
        return result;
      },
      now: () => Date.now(),
      playground: false,
    });

    const batch = Array.isArray(body) ? body : [body];
    const replies = [];
    for (const msg of batch) {
      this.pending = [];
      let reply;
      try {
        // A read needs no identity; anything else does, and says so plainly
        // rather than failing somewhere deeper with a confusing message.
        if (!me && msg?.method === 'tools/call' && !READS.has(msg?.params?.name)) {
          reply = {
            jsonrpc: '2.0', id: msg.id,
            result: { content: [{ type: 'text', text: `refused: ${NEEDS_TOKEN}` }], isError: true },
          };
        } else {
          reply = dispatch(tools, msg, { name: 'meap' });
        }
      } catch (e) {
        reply = { jsonrpc: '2.0', id: msg?.id ?? null, error: { code: -32603, message: e.message } };
      }
      // Persist whatever that message actually applied.
      for (let i = 0; i < this.pending.length; i++) {
        try {
          await this.state.storage.put(KEY(this.n), this.pending[i]);
          this.n += 1;
        } catch {
          this.ledger = null;
          return json({ jsonrpc: '2.0', id: msg?.id ?? null, error: { code: -32603, message: 'write failed; state rolled back to the durable log' } }, 500);
        }
      }
      this.pending = [];
      if (reply) replies.push(reply);
    }

    // A body of nothing but notifications gets an acknowledgement, not a reply.
    if (replies.length === 0) return new Response(null, { status: 202, headers: cors() });
    return json(Array.isArray(body) ? replies : replies[0]);
  }
}

// --- helpers ----------------------------------------------------------------

const NEEDS_TOKEN =
  'this verb acts on the ledger and needs an identity. POST /register to get a token, '
  + 'then send it as `Authorization: Bearer <token>`. Reading needs no token.';

function bearer(request) {
  const h = request.headers.get('authorization') || '';
  const m = /^Bearer\s+(.+)$/i.exec(h.trim());
  return m ? m[1].trim() : null;
}

export function cors() {
  return {
    'access-control-allow-origin': '*',
    'access-control-allow-headers': 'authorization, content-type, mcp-protocol-version, mcp-session-id',
    'access-control-allow-methods': 'GET, POST, OPTIONS',
  };
}

export function json(body, status = 200) {
  return new Response(JSON.stringify(body, null, 2), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', ...cors() },
  });
}

export { VERSION };
