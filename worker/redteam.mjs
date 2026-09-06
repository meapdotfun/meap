/**
 * Try to rob the economy.
 *
 * Every known way in, attempted for real against a running endpoint, each one
 * expected to fail. Run it after any deploy that touches money, identity or
 * genesis; a single PASS line lying about a FAIL is how the treasury bug
 * shipped, so this asserts and exits nonzero rather than describing.
 *
 * The attacks, and where each came from:
 *
 *   impersonate the treasury   shipped. Its address was derived by the same
 *                              function bearer tokens flow through, so its
 *                              label WAS a working credential.
 *   short chosen tokens        the same bug generalised: any guessable label
 *                              is somebody's address.
 *   mint                       `fund` must refuse on the shared ledger.
 *   overdraw / self-pay        the ledger's own refusals, exercised remotely.
 *   sybil the faucet           grants must taper rather than race the pot to
 *                              zero, and registering must never create money.
 *   conservation               after all of the above, total still equals
 *                              genesis supply.
 *
 *   node worker/redteam.mjs https://mcp.meap.fun
 */

import { newAgent } from './lib/signing-agent.mjs';

const BASE = (process.argv[2] || 'http://127.0.0.1:8788').replace(/\/+$/, '');

let failures = 0;
const ok = (name, cond, detail = '') => {
  console.log(`${cond ? '  ok   ' : '  FAIL '} ${name}${detail ? `  (${detail})` : ''}`);
  if (!cond) failures++;
};

const state = async () => (await fetch(`${BASE}/state`)).json();

const register = async () => newAgent(BASE);

console.log(`red team vs ${BASE}\n`);
const before = await state();
const supply = before.supply.amount;
const treasuryHeld = before.supply.held;
// The treasury is the richest thing on the ledger by construction.
const treasury = before.agents.reduce((a, b) =>
  ((a.balances.USD ?? 0) > (b.balances.USD ?? 0) ? a : b));
console.log(`supply ${supply}, treasury holds ${treasuryHeld} at ${treasury.address}\n`);

// --- 1. forge a signature or a raw request -----------------------------------

{
  // A request with a public key but a signature that does not cover it.
  const victim = await newAgent(BASE);
  const attacker = await newAgent(BASE);
  const body = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'whoami', arguments: {} } });
  const stolen = await fetch(`${BASE}/mcp`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-meap-key': victim.publicKey,           // claim to be the victim
      'x-meap-time': String(Date.now()),
      'x-meap-nonce': 'deadbeef'.repeat(4),
      'x-meap-sig': 'aa'.repeat(64),            // but a bogus signature
    },
    body,
  });
  ok('a forged signature is refused', stolen.status === 401, `status ${stolen.status}`);

  // A raw bearer header, from the removed scheme, buys nothing.
  const bearer = await fetch(`${BASE}/mcp`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: 'Bearer ' + 'f'.repeat(64) },
    body,
  });
  const bj = await bearer.json().catch(() => ({}));
  const acted = bj.result && !bj.result.isError;
  ok('a bearer token no longer acts', !acted, 'bearer is gone');
}

// --- 2. steal from it anyway -------------------------------------------------

{
  const me = await newAgent(BASE);
  await me.call('whoami', {});                 // join, take the grant
  const r = await me.call('pay', { to: me.address, asset: 'USD', amount: 1 });
  ok('paying yourself is refused', r.isError, r.text.slice(0, 40));

  const rich = await me.call('pay', { to: treasury.address, asset: 'USD', amount: 10 ** 10 });
  ok('overdrawing is refused', rich.isError, rich.text.slice(0, 50));

  for (const amount of [-5, 0, 0.5, 1e18]) {
    const bad = await me.call('pay', { to: treasury.address, asset: 'USD', amount });
    ok(`amount ${amount} is refused`, bad.isError, bad.text.slice(0, 40));
  }

  const mint = await me.call('fund', { asset: 'USD', amount: 10 ** 9 });
  ok('minting is refused on the shared ledger', mint.isError, mint.text.slice(0, 50));
}

// --- 3. sybil the faucet -----------------------------------------------------

{
  const grants = [];
  for (let i = 0; i < 5; i++) {
    const a = await newAgent(BASE);
    const w = await a.call('whoami', {});
    grants.push(w.json.balances.USD ?? 0);
  }
  const capped = grants.every((g) => g <= before.opening.amount);
  const monotone = grants.every((g, i) => i === 0 || g <= grants[i - 1]);
  ok('grants never exceed the advertised opening', capped, grants.join(','));
  ok('grants never grow as the pot shrinks', monotone);

  const now = await state();
  ok('registering created no money', now.totals.USD === supply, `${now.totals.USD} vs ${supply}`);
}

// --- 4. the books ------------------------------------------------------------

{
  const now = await state();
  ok('total equals genesis supply', now.totals.USD === now.supply.amount);
  ok('the treasury still stands', now.supply.held > 0, String(now.supply.held));
}

console.log(failures ? `\n${failures} FAILURES` : '\nnothing got in');
process.exit(failures ? 1 : 0);
