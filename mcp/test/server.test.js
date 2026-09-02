/**
 * Protocol conformance, driven the way a client drives it: a real child
 * process, real pipes, newline delimited JSON-RPC. Nothing here reaches into
 * the module graph, because the thing being tested is whether an agent that
 * knows only MCP can actually use this.
 *
 * Run: npm test
 */

import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve as resolvePath } from 'node:path';
import { fileURLToPath } from 'node:url';

const SERVER = fileURLToPath(new URL('../src/server.js', import.meta.url));
const dir = mkdtempSync(join(tmpdir(), 'meap-test-'));
after(() => { try { rmSync(dir, { recursive: true, force: true }); } catch { /* windows lock */ } });

/** A minimal MCP client: writes requests, resolves responses by id. */
class Client {
  constructor(agent, state, env = {}) {
    this.proc = spawn(process.execPath, [SERVER], {
      env: { ...process.env, MEAP_AGENT: agent, MEAP_STATE: state, ...env },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    this.id = 0;
    this.pending = new Map();
    this.stderr = '';
    this.buf = '';
    this.proc.stdout.setEncoding('utf8');
    this.proc.stderr.setEncoding('utf8');
    this.proc.stderr.on('data', (d) => { this.stderr += d; });
    this.proc.stdout.on('data', (d) => {
      this.buf += d;
      let i;
      while ((i = this.buf.indexOf('\n')) >= 0) {
        const line = this.buf.slice(0, i);
        this.buf = this.buf.slice(i + 1);
        if (!line.trim()) continue;
        const msg = JSON.parse(line);
        const p = this.pending.get(msg.id);
        if (p) { this.pending.delete(msg.id); p(msg); }
      }
    });
  }

  send(method, params) {
    const id = ++this.id;
    return new Promise((res, rej) => {
      const timer = setTimeout(() => rej(new Error(`timed out on ${method}; stderr: ${this.stderr}`)), 10_000);
      this.pending.set(id, (m) => { clearTimeout(timer); res(m); });
      this.proc.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    });
  }

  notify(method, params) {
    this.proc.stdin.write(JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n');
  }

  /** Call a tool and parse the JSON its text content carries. */
  async call(name, args) {
    const r = await this.send('tools/call', { name, arguments: args });
    const text = r.result.content[0].text;
    return { isError: !!r.result.isError, text, json: safeJson(text) };
  }

  async handshake() {
    const r = await this.send('initialize', {
      protocolVersion: '2025-06-18',
      capabilities: {},
      clientInfo: { name: 'test', version: '0' },
    });
    this.notify('notifications/initialized');
    return r;
  }

  close() { this.proc.stdin.end(); this.proc.kill(); }
}

const safeJson = (t) => { try { return JSON.parse(t); } catch { return null; } };

// --- protocol ---------------------------------------------------------------

test('initialize, list and call complete a normal session', async () => {
  const state = join(dir, 'session.log');
  const c = new Client('alice', state);
  try {
    const init = await c.handshake();
    assert.equal(init.result.serverInfo.name, 'meap');
    assert.equal(init.result.protocolVersion, '2025-06-18');
    assert.ok(init.result.capabilities.tools, 'the server advertises tools');

    const list = await c.send('tools/list');
    const names = list.result.tools.map((t) => t.name);
    for (const verb of ['create_market', 'post_offer', 'accept_offer', 'repay', 'foreclose', 'attest', 'settle', 'hire']) {
      assert.ok(names.includes(verb), `${verb} must be callable`);
    }
    for (const t of list.result.tools) {
      assert.equal(typeof t.description, 'string');
      assert.equal(t.inputSchema.type, 'object', `${t.name} needs an object schema`);
    }

    const me = await c.call('whoami');
    assert.match(me.json.address, /^ag_[0-9a-f]{32}$/);
    assert.equal(me.json.stakes, 'off');

    const pong = await c.send('ping');
    assert.deepEqual(pong.result, {});
  } finally { c.close(); }
});

test('an unknown method is a protocol error, an unknown tool is not', async () => {
  const c = new Client('alice', join(dir, 'errs.log'));
  try {
    await c.handshake();
    const bad = await c.send('resources/list');
    assert.equal(bad.error.code, -32601);

    const nope = await c.call('teleport', {});
    assert.ok(nope.isError);
    assert.match(nope.text, /no such tool/);
  } finally { c.close(); }
});

test('a refusal reaches the agent as readable text rather than a crash', async () => {
  const c = new Client('alice', join(dir, 'refuse.log'));
  try {
    await c.handshake();
    const r = await c.call('create_market', { declaration: { collateral: { asset: 'USD' } } });
    assert.ok(r.isError);
    assert.match(r.text, /refused:/);
    // The server is still alive and serving after refusing.
    const me = await c.call('whoami');
    assert.ok(me.json.address);
  } finally { c.close(); }
});

// --- the verbs, over the wire ------------------------------------------------

test('two agents open a loan through the protocol and it forecloses', async () => {
  const state = join(dir, 'loan.log');
  const borrower = new Client('borrower', state);
  try {
    await borrower.handshake();
    await borrower.call('fund', { asset: 'USD', amount: 500_000 });

    const now = Date.now();
    const decl = await borrower.call('create_market', {
      declaration: {
        collateral: { asset: 'USD' },
        positions: { kind: 'categorical', outcomes: ['REPAID', 'DEFAULTED'] },
        resolution: { kind: 'deadline', at: now + 1_000 },
        payoff: { kind: 'seizure', to: 'DEFAULTED', discharge: 110_000 },
        mechanism: { kind: 'bilateral' },
        expiry: now + 60_000,
        label: 'a thousand for thirty days',
      },
    });
    assert.ok(!decl.isError, decl.text);
    const market = decl.json.market;

    const offer = await borrower.call('post_offer', {
      market, leg: 'REPAID', stake: 150_000, ask: 100_000, counter_stake: 0,
    });
    assert.ok(!offer.isError, offer.text);
    borrower.close();

    // A different agent, same ledger file, connecting fresh. Its view is
    // whatever replaying the log produces.
    const lender = new Client('lender', state);
    try {
      await lender.handshake();
      await lender.call('fund', { asset: 'USD', amount: 500_000 });

      const offers = await lender.call('list_offers', { market });
      assert.equal(offers.json.length, 1, 'the offer survived a restart');

      const taken = await lender.call('accept_offer', { offer: offer.json.offer });
      assert.ok(!taken.isError, taken.text);
      assert.equal(taken.json.youHold.leg, 'DEFAULTED');

      // Too early: the obligation has not come due.
      const early = await lender.call('foreclose', { market });
      assert.ok(early.isError);
      assert.match(early.text, /not ready to settle/);

      await new Promise((r) => setTimeout(r, 1_200));

      const seized = await lender.call('foreclose', { market });
      assert.ok(!seized.isError, seized.text);
      assert.equal(seized.json.state, 'defaulted');

      const audit = await lender.call('audit');
      assert.equal(audit.json.totals.USD, 1_000_000, 'nothing was created or lost');
      assert.match(audit.json.digest, /^[0-9a-f]{32}$/);
    } finally { lender.close(); }
  } finally { borrower.close(); }
});

test('the log on disk is the state, and it replays to the same digest', async () => {
  const state = join(dir, 'replay.log');
  const a = new Client('alice', state);
  let digest;
  try {
    await a.handshake();
    await a.call('fund', { asset: 'USD', amount: 10_000 });
    await a.call('create_market', {
      declaration: {
        collateral: { asset: 'USD' },
        positions: { kind: 'binary' },
        resolution: { kind: 'attestation', by: [(await a.call('whoami')).json.address], quorum: 1 },
        payoff: { kind: 'winner_take_all' },
        mechanism: { kind: 'bilateral' },
        expiry: Date.now() + 60_000,
      },
    });
    digest = (await a.call('audit')).json.digest;
  } finally { a.close(); }

  assert.ok(existsSync(state));
  const lines = readFileSync(state, 'utf8').trim().split('\n');
  assert.ok(lines.length >= 3, 'every action was journalled');
  for (const l of lines) assert.ok(JSON.parse(l).at > 0, 'each action carries its own timestamp');

  // A fresh process over the same log must land on the same state.
  const b = new Client('alice', state);
  try {
    await b.handshake();
    assert.equal((await b.call('audit')).json.digest, digest, 'replay is not reproducing the state');
  } finally { b.close(); }
});

test('with stakes on there is no way to create value', async () => {
  const c = new Client('alice', join(dir, 'live.log'), { MEAP_STAKES: 'on' });
  try {
    await c.handshake();
    const r = await c.call('fund', { asset: 'USD', amount: 1_000_000 });
    assert.ok(r.isError);
    assert.match(r.text, /playground only/);
  } finally { c.close(); }
});

test('the server refuses to start without an identity', async () => {
  const code = await new Promise((res) => {
    const p = spawn(process.execPath, [SERVER], {
      env: { ...process.env, MEAP_AGENT: '', MEAP_STATE: join(dir, 'noid.log') },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    p.on('exit', res);
  });
  assert.equal(code, 2, 'no identity, no connection');
});
