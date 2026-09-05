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

const BASE = (process.argv[2] || 'http://127.0.0.1:8788').replace(/\/+$/, '');

let failures = 0;
const ok = (name, cond, detail = '') => {
  console.log(`${cond ? '  ok   ' : '  FAIL '} ${name}${detail ? `  (${detail})` : ''}`);
  if (!cond) failures++;
};

const state = async () => (await fetch(`${BASE}/state`)).json();

async function call(token, name, args) {
  const r = await fetch(`${BASE}/mcp`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name, arguments: args } }),
  });
  const body = await r.json().catch(() => ({}));
  return {
    status: r.status,
    isError: !!body.result?.isError || !!body.error,
    text: body.result?.content?.[0]?.text ?? body.error?.message ?? '',
  };
}

const register = async () => (await fetch(`${BASE}/register`, { method: 'POST' })).json();

console.log(`red team vs ${BASE}\n`);
const before = await state();
const supply = before.supply.amount;
const treasuryHeld = before.supply.held;
// The treasury is the richest thing on the ledger by construction.
const treasury = before.agents.reduce((a, b) =>
  ((a.balances.USD ?? 0) > (b.balances.USD ?? 0) ? a : b));
console.log(`supply ${supply}, treasury holds ${treasuryHeld} at ${treasury.address}\n`);

// --- 1. become the treasury --------------------------------------------------

{
  const r = await call('meap:treasury:v1', 'whoami', {});
  ok('the old treasury label is refused outright', r.status === 401, r.text.slice(0, 60));

  // Padded guesses clear the length floor but must land on fresh addresses.
  for (const guess of ['system:treasury'.padEnd(32, 'x'), 'meap:treasury:v1:aaaaaaaaaaaaaaaa', 'treasury'.repeat(4)]) {
    const w = await call(guess, 'whoami', {});
    const addr = w.isError ? null : JSON.parse(w.text).address;
    ok(`token ${JSON.stringify(guess.slice(0, 20))}... is not the treasury`, addr !== treasury.address, addr ?? w.text.slice(0, 40));
  }
}

// --- 2. steal from it anyway -------------------------------------------------

{
  const me = await register();
  const r = await call(me.token, 'pay', { to: me.address, asset: 'USD', amount: 1 });
  ok('paying yourself is refused', r.isError, r.text.slice(0, 40));

  const rich = await call(me.token, 'pay', { to: treasury.address, asset: 'USD', amount: 10 ** 10 });
  ok('overdrawing is refused', rich.isError, rich.text.slice(0, 50));

  for (const amount of [-5, 0, 0.5, 1e18]) {
    const bad = await call(me.token, 'pay', { to: treasury.address, asset: 'USD', amount });
    ok(`amount ${amount} is refused`, bad.isError, bad.text.slice(0, 40));
  }

  const mint = await call(me.token, 'fund', { asset: 'USD', amount: 10 ** 9 });
  ok('minting is refused on the shared ledger', mint.isError, mint.text.slice(0, 50));
}

// --- 3. sybil the faucet -----------------------------------------------------

{
  const grants = [];
  for (let i = 0; i < 5; i++) {
    const s = await register();
    const w = await call(s.token, 'whoami', {});
    grants.push(JSON.parse(w.text).balances.USD ?? 0);
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
