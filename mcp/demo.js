/**
 * One economy, played out.
 *
 * A borrower opens a loan. A third party who was not asked writes insurance
 * against that loan defaulting. The loan defaults. A fourth agent, who lent
 * nothing to anyone, forecloses it for the bounty. The insurance can only then
 * read its answer, and pays.
 *
 * Timestamps are fixed rather than read from the clock, so this prints the same
 * digest on every machine on every run. That is the point of the ledger being
 * replayable: the numbers below are checkable, not claimed.
 *
 * Run: npm run demo
 */

import { Ledger, addressOf } from './src/ledger.js';

const DAY = 86_400_000;
const T0 = 1_756_000_000_000;

const A = { borrower: addressOf('borrower'), lender: addressOf('lender'), insurer: addressOf('insurer'), watcher: addressOf('watcher') };
const short = (x) => (typeof x === 'string' && x.length > 20 ? `${x.slice(0, 9)}…` : x);
const usd = (n) => `${(n / 100).toFixed(2)}`;

const l = new Ledger({ playground: true });
let step = 0;

function run(label, action) {
  const out = l.apply(action);
  step += 1;
  console.log(`\n${String(step).padStart(2)}. ${label}`);
  console.log(`    ${action.type}  by ${short(action.by)}`);
  const shown = Object.entries(out)
    .filter(([k]) => !['seq'].includes(k))
    .map(([k, v]) => `${k}=${typeof v === 'object' ? JSON.stringify(v, (kk, vv) => (typeof vv === 'string' ? short(vv) : vv)) : short(v)}`);
  if (shown.length) console.log(`    -> ${shown.join('  ')}`);
  return out;
}

console.log('MEAP demo: a loan, insurance written against it, and a default.\n');
console.log('Amounts are minor units. 150000 is fifteen hundred dollars.');

for (const [name, addr] of Object.entries(A)) {
  l.apply({ type: 'join', by: addr, at: T0 });
  l.apply({ type: 'mint', by: addr, asset: 'USD', amount: 1_000_000, at: T0 });
  console.log(`    ${name.padEnd(9)} ${addr}  ${usd(1_000_000)}`);
}
const opening = l.audit().get('USD');

// --- the loan ---------------------------------------------------------------

const loan = run('the borrower declares a loan: collateral seized if the deadline passes undischarged', {
  type: 'create_market', by: A.borrower, at: T0,
  declaration: {
    collateral: { asset: 'USD' },
    positions: { kind: 'categorical', outcomes: ['REPAID', 'DEFAULTED'] },
    resolution: { kind: 'deadline', at: T0 + 30 * DAY },
    payoff: { kind: 'seizure', to: 'DEFAULTED', discharge: 110_000 },
    mechanism: { kind: 'bilateral' },
    expiry: T0 + 31 * DAY,
    label: 'borrow 1000 against 1500 collateral, thirty days, repay 1100',
  },
}).market;

const offer = run('it escrows the collateral and asks for the principal, requiring nothing back', {
  type: 'post_offer', by: A.borrower, market: loan, leg: 'REPAID',
  stake: 150_000, ask: 100_000, counter_stake: 0, at: T0,
}).offer;

run('a lender takes the other side, paying the principal directly', {
  type: 'accept_offer', by: A.lender, offer, at: T0,
});

// --- the derivative ---------------------------------------------------------

const cover = run('a third party writes insurance on that loan defaulting; nobody asked it to', {
  type: 'create_market', by: A.insurer, at: T0,
  declaration: {
    collateral: { asset: 'USD' },
    positions: { kind: 'binary' },
    resolution: { kind: 'market', market: loan, when: 'defaulted' },
    payoff: { kind: 'winner_take_all' },
    mechanism: { kind: 'lmsr', b: 20_000 },
    expiry: T0 + 40 * DAY,
    label: 'pays if the loan above defaults',
  },
}).market;

run('a watcher buys the cover, betting the borrower will not repay', {
  type: 'buy', by: A.watcher, market: cover, leg: 'YES', shares: 30_000, at: T0 + DAY,
});

// --- the default ------------------------------------------------------------

console.log('\n    thirty days pass. nothing is repaid.');
const at = T0 + 31 * DAY;

run('anyone may close out an obligation that has come due, and is paid for it', {
  type: 'settle', by: A.watcher, market: loan, at,
});

run('only now can the cover read its answer', {
  type: 'settle', by: A.insurer, market: cover, at,
});

// --- the record -------------------------------------------------------------

console.log('\n--- final ---\n');
for (const [name, addr] of Object.entries(A)) {
  const d = l.balance(addr, 'USD') - 1_000_000;
  console.log(`    ${name.padEnd(9)} ${usd(l.balance(addr, 'USD')).padStart(10)}   ${d >= 0 ? '+' : ''}${usd(d)}`);
}

const closing = l.audit().get('USD');
console.log(`\n    total before ${usd(opening)}   after ${usd(closing)}   ${opening === closing ? 'conserved' : 'LEAKED'}`);
console.log(`    ${l.log.length} actions, ${l.markets.size} markets, digest ${l.digest()}`);

const replayed = Ledger.replay(l.log, { playground: true });
console.log(`    replayed from the log: ${replayed.digest()}  ${replayed.digest() === l.digest() ? 'identical' : 'DIVERGED'}`);

if (closing !== opening || replayed.digest() !== l.digest()) process.exit(1);
