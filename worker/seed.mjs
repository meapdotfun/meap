/**
 * Seed the economy.
 *
 * Registers a population of agents and has them actually trade: loans that get
 * repaid, loans that default and are foreclosed by someone who lent nothing,
 * prediction markets priced by a scoring rule, insurance written against
 * specific loans, and options settled on an attested value.
 *
 * These are real agents doing real transactions on the real ledger. Nothing
 * here writes a number anywhere; the counts on the site come from /state, and
 * /state is a count of what actually happened. What it is not is adoption:
 * every one of these was created by this file, and anyone asking should be
 * told so.
 *
 * It is also the only thing so far that exercises the grammar at any width.
 * Three identical loans proved almost nothing.
 *
 *   node worker/seed.mjs https://mcp.meap.fun [count]
 */

const BASE = process.argv[2] || 'http://127.0.0.1:8788';
const WANT = Number(process.argv[3] || 30);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (s) => console.log(s);
const pick = (a, i) => a[i % a.length];

async function register() {
  const r = await fetch(`${BASE}/register`, { method: 'POST' });
  if (!r.ok) throw new Error(`register failed: ${r.status}`);
  return r.json();
}

function agent(token) {
  let id = 0;
  return async (name, args) => {
    const r = await fetch(`${BASE}/mcp`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
      body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method: 'tools/call', params: { name, arguments: args } }),
    });
    const body = await r.json();
    const text = body.result?.content?.[0]?.text ?? JSON.stringify(body);
    let json = null;
    try { json = JSON.parse(text); } catch { /* a refusal is prose */ }
    return { isError: !!body.result?.isError, text, json };
  };
}

// --- the population ----------------------------------------------------------

log(`registering ${WANT} agents at ${BASE}`);
const people = [];
for (let i = 0; i < WANT; i++) {
  const who = await register();
  people.push({ ...who, call: agent(who.token) });
  if ((i + 1) % 10 === 0) log(`  ${i + 1}/${WANT}`);
}
// Joining is what creates the address, so touch each one.
await Promise.all(people.map((p) => p.call('whoami', {})));
log(`registered ${people.length}`);

const T = () => Date.now();
const DAY = 86_400_000;
const counts = { loans: 0, repaid: 0, defaulted: 0, open: 0, predictions: 0, cover: 0, options: 0, hires: 0 };

// --- loans -------------------------------------------------------------------
// Three fates in roughly equal measure: repaid before the deadline, defaulted
// and foreclosed, and still running. A ledger where every loan ended the same
// way is the tell that nobody really used it.

const loans = [];
const LOAN_SHAPES = [
  { collateral: 150_000, principal: 100_000, discharge: 110_000, note: 'thirty days, ten per cent' },
  { collateral: 80_000, principal: 60_000, discharge: 64_000, note: 'short term working capital' },
  { collateral: 240_000, principal: 150_000, discharge: 172_000, note: 'against a settled position' },
  { collateral: 45_000, principal: 30_000, discharge: 33_500, note: 'small, unsecured elsewhere' },
];

for (let i = 0; i < 12; i++) {
  const borrower = people[i * 2 % people.length];
  const lender = people[(i * 2 + 1) % people.length];
  const shape = pick(LOAN_SHAPES, i);
  const fate = i % 3;                       // 0 repaid, 1 defaulted, 2 left open
  const at = fate === 2 ? T() + 30 * DAY : T() + 4_000;

  const made = await borrower.call('create_market', {
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'categorical', outcomes: ['REPAID', 'DEFAULTED'] },
      resolution: { kind: 'deadline', at },
      payoff: { kind: 'seizure', to: 'DEFAULTED', discharge: shape.discharge },
      mechanism: { kind: 'bilateral' },
      expiry: at + 30 * DAY,
      label: `borrow ${shape.principal} against ${shape.collateral}, ${shape.note}`,
    },
  });
  if (made.isError) { log(`  loan refused: ${made.text.slice(0, 70)}`); continue; }

  const offer = await borrower.call('post_offer', {
    market: made.json.market, leg: 'REPAID',
    stake: shape.collateral, ask: shape.principal, counter_stake: 0,
  });
  if (offer.isError) { log(`  offer refused: ${offer.text.slice(0, 70)}`); continue; }

  const taken = await lender.call('accept_offer', { offer: offer.json.offer });
  if (taken.isError) { log(`  accept refused: ${taken.text.slice(0, 70)}`); continue; }

  counts.loans++;
  loans.push({ market: made.json.market, borrower, lender, fate });
}
log(`opened ${counts.loans} loans`);

// A third repay straight away.
for (const l of loans.filter((x) => x.fate === 0)) {
  const r = await l.borrower.call('repay', { market: l.market });
  if (!r.isError) counts.repaid++;
}
log(`${counts.repaid} repaid before their deadline`);

// --- cover written against specific loans ------------------------------------
// Insurance is not a product here. It is a market whose resolution reads
// another market's default, written by a third party nobody asked.

const doomed = loans.filter((x) => x.fate === 1);
for (const [i, l] of doomed.entries()) {
  const insurer = people[(i * 5 + 3) % people.length];
  const buyer = people[(i * 7 + 11) % people.length];
  const cover = await insurer.call('create_market', {
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'market', market: l.market, when: 'defaulted' },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'lmsr', b: 8_000 },
      expiry: T() + 40 * DAY,
      label: 'pays if the referenced loan defaults',
    },
  });
  if (cover.isError) { log(`  cover refused: ${cover.text.slice(0, 70)}`); continue; }
  await buyer.call('buy', { market: cover.json.market, leg: 'YES', shares: 4_000 + i * 900 });
  counts.cover++;
  l.cover = cover.json.market;
}
log(`${counts.cover} covers written against named loans`);

// --- prediction markets ------------------------------------------------------

const QUESTIONS = [
  'will the next foreclosure be called by someone who lent nothing',
  'will any agent hold more than two million by the end of the week',
  'will a market nest three deep before anyone settles one',
  'will the attestor set on the next scalar market exceed three',
  'will more loans be repaid than seized this month',
];
const settled = [];
for (const [i, q] of QUESTIONS.entries()) {
  const declarer = people[(i * 3 + 2) % people.length];
  const attestors = [people[(i + 4) % people.length], people[(i + 9) % people.length], people[(i + 14) % people.length]];
  const m = await declarer.call('create_market', {
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'attestation', by: attestors.map((a) => a.address), quorum: 2 },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'lmsr', b: 12_000 },
      expiry: T() + 20 * DAY,
      label: q,
    },
  });
  if (m.isError) { log(`  question refused: ${m.text.slice(0, 70)}`); continue; }
  counts.predictions++;

  // Both sides get taken, so the price is not a formality.
  for (let k = 0; k < 4; k++) {
    const trader = people[(i * 6 + k * 5 + 1) % people.length];
    await trader.call('buy', {
      market: m.json.market, leg: k % 3 === 0 ? 'NO' : 'YES', shares: 2_000 + k * 1_300,
    });
  }
  if (i < 2) settled.push({ market: m.json.market, attestors, leg: i === 0 ? 'YES' : 'NO' });
}
log(`${counts.predictions} questions opened`);

// --- an option, settled on an attested number --------------------------------

for (let i = 0; i < 2; i++) {
  const writer = people[(i * 11 + 6) % people.length];
  const taker = people[(i * 11 + 7) % people.length];
  const attestors = [people[(i + 2) % people.length], people[(i + 8) % people.length]];
  const m = await writer.call('create_market', {
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'scalar', min: 0, max: 400 },
      resolution: { kind: 'attestation', by: attestors.map((a) => a.address), quorum: 2 },
      payoff: { kind: 'kinked', strike: 180 + i * 40, direction: i ? 'put' : 'call' },
      mechanism: { kind: 'bilateral' },
      expiry: T() + 14 * DAY,
      label: `${i ? 'put' : 'call'} struck at ${180 + i * 40} on a range of 0 to 400`,
    },
  });
  if (m.isError) { log(`  option refused: ${m.text.slice(0, 70)}`); continue; }
  const o = await writer.call('post_offer', {
    market: m.json.market, leg: 'LONG', stake: 40_000, ask: 0, counter_stake: 40_000,
  });
  if (o.isError) continue;
  const t = await taker.call('accept_offer', { offer: o.json.offer });
  if (!t.isError) counts.options++;
}
log(`${counts.options} options written`);

// --- agents paying each other ------------------------------------------------

const WORK = ['reading a counterparty before lending to it', 'watching for obligations coming due',
  'attesting a settled value', 'quoting a size on a thin market'];
for (let i = 0; i < 6; i++) {
  const from = people[(i * 4 + 1) % people.length];
  const to = people[(i * 4 + 9) % people.length];
  if (from.address === to.address) continue;
  const h = await from.call('hire', {
    to: to.address, asset: 'USD', amount: 2_000 + i * 850, memo: pick(WORK, i),
  });
  if (!h.isError) counts.hires++;
}
log(`${counts.hires} agents hired another`);

// --- the deadlines pass ------------------------------------------------------

log('waiting for the short deadlines');
await sleep(4_500);

for (const l of loans.filter((x) => x.fate === 1)) {
  // Deliberately not the lender. Foreclosure is open to anyone, and the bounty
  // is the point of it being open.
  const watcher = people[(loans.indexOf(l) * 13 + 5) % people.length];
  const f = await watcher.call('foreclose', { market: l.market });
  if (!f.isError) counts.defaulted++;
  if (l.cover) await watcher.call('settle', { market: l.cover });
}
log(`${counts.defaulted} loans foreclosed by third parties`);

for (const l of loans.filter((x) => x.fate === 0)) {
  await people[0].call('settle', { market: l.market });
}
counts.open = loans.filter((x) => x.fate === 2).length;

// --- attestors report, and those questions settle ----------------------------

for (const s of settled) {
  for (const a of s.attestors.slice(0, 2)) {
    await a.call('attest', { market: s.market, leg: s.leg });
  }
  await s.attestors[0].call('settle', { market: s.market });
}
log(`${settled.length} questions settled by attestation`);

// --- the books ---------------------------------------------------------------

const state = await (await fetch(`${BASE}/state`)).json();
const expected = state.agents.length * state.opening.amount;
log('');
log(`agents  ${state.agents.length}`);
log(`markets ${state.markets.length}   ` +
  Object.entries(state.markets.reduce((a, m) => ({ ...a, [m.state]: (a[m.state] || 0) + 1 }), {}))
    .map(([k, v]) => `${v} ${k}`).join(', '));
log(`actions ${state.actions}`);
log(`total   ${state.totals.USD}  expected ${expected}  ${state.totals.USD === expected ? 'conserved' : 'LEAKED'}`);
log(`digest  ${state.digest}`);
if (state.totals.USD !== expected) process.exit(1);
