/**
 * Ledger behaviour, and the scenario the whole design exists for: a loan, an
 * insurance market written against that loan by a third party, the loan
 * defaulting, and both settling in order.
 *
 * Amounts are minor units. A loan of 100000 is a thousand dollars.
 *
 * Run: npm test
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';

import { Ledger, addressOf, quote } from '../src/ledger.js';
import { prices, costToBuy, maxSubsidy } from '../src/lmsr.js';
import { proRata, median } from '../src/settle.js';

const DAY = 86_400_000;
const T0 = 1_756_000_000_000;

const BORROWER = addressOf('borrower');
const LENDER = addressOf('lender');
const INSURER = addressOf('insurer');
const WATCHER = addressOf('watcher');
const ORACLE = addressOf('oracle');

/** A funded playground ledger with the named agents present. */
function world(funding = 1_000_000) {
  const l = new Ledger({ playground: true });
  for (const who of [BORROWER, LENDER, INSURER, WATCHER, ORACLE]) {
    l.apply({ type: 'join', by: who, at: T0 });
    l.apply({ type: 'mint', by: who, asset: 'USD', amount: funding, at: T0 });
  }
  return l;
}

/** Sum of every balance and every escrow. Must not move except on a mint. */
const total = (l) => l.audit().get('USD') ?? 0;

const loanDeclaration = (at = T0) => ({
  collateral: { asset: 'USD' },
  positions: { kind: 'categorical', outcomes: ['REPAID', 'DEFAULTED'] },
  resolution: { kind: 'deadline', at: at + 30 * DAY },
  payoff: { kind: 'seizure', to: 'DEFAULTED', discharge: 110_000 },
  mechanism: { kind: 'bilateral' },
  expiry: at + 31 * DAY,
  label: 'borrow 100000 against 150000, thirty days',
});

/** Open a loan: borrower escrows collateral, lender pays the principal. */
function openLoan(l, at = T0) {
  const { market } = l.apply({ type: 'create_market', by: BORROWER, declaration: loanDeclaration(at), at });
  const { offer } = l.apply({
    type: 'post_offer', by: BORROWER, market, leg: 'REPAID',
    stake: 150_000, ask: 100_000, counter_stake: 0, at,
  });
  l.apply({ type: 'accept_offer', by: LENDER, offer, at });
  return market;
}

// --- the scenario -----------------------------------------------------------

test('a loan defaults and insurance written against it pays out', () => {
  const l = world();
  const opening = total(l);

  const loan = openLoan(l);

  // The lender handed over the principal and holds a claim on the collateral.
  assert.equal(l.balance(BORROWER, 'USD'), 1_000_000 - 150_000 + 100_000);
  assert.equal(l.balance(LENDER, 'USD'), 1_000_000 - 100_000);
  assert.equal(l.markets.get(loan).escrow, 150_000);
  assert.equal(total(l), opening, 'opening a loan moves value, it does not create it');

  // A third party writes insurance on that loan defaulting. Nobody asked the
  // borrower or the lender; the loan is public and referencing it is open.
  const b = 10_000;
  const { market: cover, seeded } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'market', market: loan, when: 'defaulted' },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'lmsr', b },
      expiry: T0 + 40 * DAY,
      label: 'pays if the loan defaults',
    },
  });
  assert.equal(seeded, maxSubsidy(b, 2), 'the declarer posts the mechanism subsidy up front');

  const bought = l.apply({ type: 'buy', by: WATCHER, market: cover, leg: 'YES', shares: 5_000, at: T0 });
  assert.ok(bought.paid > 0);
  assert.equal(total(l), opening);

  // The cover cannot settle while the loan is still open.
  assert.throws(() => l.apply({ type: 'settle', by: WATCHER, market: cover, at: T0 + DAY }),
    /referenced market has not settled/);

  // Thirty days pass and nothing was repaid. Anyone may close it out.
  const at = T0 + 31 * DAY;
  const fore = l.apply({ type: 'settle', by: WATCHER, market: loan, at });
  assert.equal(fore.state, 'defaulted');
  assert.equal(fore.outcome.leg, 'DEFAULTED');
  assert.ok(fore.bounty > 0, 'closing out a due obligation is paid work');
  assert.equal(fore.payouts[LENDER], 150_000 - fore.bounty, 'the lender takes the collateral');
  assert.equal(l.markets.get(loan).escrow, 0);

  // Only now can the cover read its answer.
  const paid = l.apply({ type: 'settle', by: INSURER, market: cover, at });
  assert.equal(paid.state, 'settled');
  assert.equal(paid.outcome.leg, 'YES');
  assert.ok(paid.payouts[WATCHER] > 0, 'the cover pays because the loan defaulted');

  assert.equal(total(l), opening, 'value is conserved across the whole scenario');
  assert.equal(l.balance(BORROWER, 'USD'), 950_000, 'the borrower kept the principal and lost the collateral');
});

test('the same loan repaid in time returns the collateral', () => {
  const l = world();
  const opening = total(l);
  const loan = openLoan(l);

  const r = l.apply({ type: 'repay', by: BORROWER, market: loan, at: T0 + 10 * DAY });
  assert.equal(r.paid, 110_000);
  assert.deepEqual(r.to, [LENDER], 'the discharge goes straight to the seizing leg');

  const s = l.apply({ type: 'settle', by: WATCHER, market: loan, at: T0 + 31 * DAY });
  assert.equal(s.state, 'settled', 'a discharged loan settles rather than defaulting');
  assert.equal(s.outcome.leg, 'REPAID');
  assert.ok(s.payouts[BORROWER] > 0, 'the collateral comes back');

  // Borrower: -150000 collateral, +100000 principal, -110000 discharge,
  // +150000 collateral back less the bounty. Lender is up the interest.
  assert.equal(l.balance(LENDER, 'USD'), 1_000_000 - 100_000 + 110_000 + s.bounty * 0);
  assert.ok(l.balance(BORROWER, 'USD') < 1_000_000, 'interest was paid');
  assert.equal(total(l), opening);
});

test('a loan cannot be repaid after the deadline, only foreclosed', () => {
  const l = world();
  const loan = openLoan(l);
  assert.throws(() => l.apply({ type: 'repay', by: BORROWER, market: loan, at: T0 + 31 * DAY }),
    /deadline has passed/);
});

test('insurance on a loan that was repaid pays nothing', () => {
  const l = world();
  const loan = openLoan(l);
  const { market: cover } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'market', market: loan, when: 'defaulted' },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'lmsr', b: 10_000 },
      expiry: T0 + 40 * DAY,
    },
  });
  l.apply({ type: 'buy', by: WATCHER, market: cover, leg: 'YES', shares: 5_000, at: T0 });

  l.apply({ type: 'repay', by: BORROWER, market: loan, at: T0 + DAY });
  l.apply({ type: 'settle', by: WATCHER, market: loan, at: T0 + 31 * DAY });
  const out = l.apply({ type: 'settle', by: INSURER, market: cover, at: T0 + 31 * DAY });

  assert.equal(out.outcome.leg, 'NO', 'the referenced condition did not hold');
  assert.equal(out.payouts[WATCHER] ?? 0, 0, 'a cover against a default that never came is worthless');
});

// --- attestation ------------------------------------------------------------

test('attestors who match the outcome are paid and the rest are not', () => {
  const l = world();
  const liar = addressOf('liar');
  l.apply({ type: 'join', by: liar, at: T0 });
  l.apply({ type: 'mint', by: liar, asset: 'USD', amount: 10, at: T0 });

  const { market } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'attestation', by: [ORACLE, WATCHER, liar], quorum: 2 },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'lmsr', b: 40_000 },
      expiry: T0 + 10 * DAY,
    },
  });
  l.apply({ type: 'buy', by: BORROWER, market, leg: 'YES', shares: 20_000, at: T0 });

  const before = { oracle: l.balance(ORACLE, 'USD'), liar: l.balance(liar, 'USD') };

  l.apply({ type: 'attest', by: ORACLE, market, leg: 'YES', at: T0 + DAY });
  assert.throws(() => l.apply({ type: 'settle', by: WATCHER, market, at: T0 + DAY }),
    /1 of 2 attestations in/);

  l.apply({ type: 'attest', by: liar, market, leg: 'NO', at: T0 + DAY });
  l.apply({ type: 'attest', by: WATCHER, market, leg: 'YES', at: T0 + DAY });

  const s = l.apply({ type: 'settle', by: INSURER, market, at: T0 + DAY });
  assert.equal(s.outcome.leg, 'YES');
  assert.ok(s.attestPool > 0);
  assert.ok(l.balance(ORACLE, 'USD') > before.oracle, 'reporting the outcome is paid work');
  assert.equal(l.balance(liar, 'USD'), before.liar, 'reporting against it earns nothing');
});

test('only named attestors may report, and only once', () => {
  const l = world();
  const { market } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'attestation', by: [ORACLE], quorum: 1 },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'bilateral' },
      expiry: T0 + DAY,
    },
  });
  assert.throws(() => l.apply({ type: 'attest', by: WATCHER, market, leg: 'YES', at: T0 }),
    /not an attestor/);
  l.apply({ type: 'attest', by: ORACLE, market, leg: 'YES', at: T0 });
  assert.throws(() => l.apply({ type: 'attest', by: ORACLE, market, leg: 'NO', at: T0 }),
    /already attested/);
});

test('a scalar market settles on the median, so one outlier moves nothing', () => {
  const l = world();
  const { market } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'scalar', min: 0, max: 400 },
      resolution: { kind: 'attestation', by: [ORACLE, WATCHER, LENDER], quorum: 3 },
      payoff: { kind: 'kinked', strike: 200, direction: 'call' },
      mechanism: { kind: 'bilateral' },
      expiry: T0 + DAY,
      label: 'call struck at 200',
    },
  });
  const { offer } = l.apply({
    type: 'post_offer', by: BORROWER, market, leg: 'LONG',
    stake: 50_000, ask: 0, counter_stake: 50_000, at: T0,
  });
  l.apply({ type: 'accept_offer', by: INSURER, offer, at: T0 });

  l.apply({ type: 'attest', by: ORACLE, market, value: 300, at: T0 });
  l.apply({ type: 'attest', by: WATCHER, market, value: 300, at: T0 });
  l.apply({ type: 'attest', by: LENDER, market, value: 40_000, at: T0 });

  const s = l.apply({ type: 'settle', by: WATCHER, market, at: T0 });
  assert.equal(s.outcome.value, 300, 'the median ignores the outlier');
  // A call struck at 200 on a range topping out at 400, settling at 300, is
  // exactly half in the money, so the pot splits evenly.
  assert.ok(Math.abs(s.payouts[BORROWER] - s.payouts[INSURER]) <= 1);
});

// --- conservation, determinism -----------------------------------------------

test('value is conserved after every single action', () => {
  const l = world();
  const opening = total(l);
  const loan = openLoan(l);
  const steps = [
    { type: 'buy', by: WATCHER, market: null, leg: 'YES', shares: 1_000, at: T0 },
  ];
  // Walk the loan scenario one action at a time, auditing between each.
  l.apply({ type: 'repay', by: BORROWER, market: loan, at: T0 + DAY });
  assert.equal(total(l), opening);
  l.apply({ type: 'settle', by: WATCHER, market: loan, at: T0 + 31 * DAY });
  assert.equal(total(l), opening);
  assert.equal(steps.length, 1);
});

test('replaying the action log reproduces the state exactly', () => {
  const l = world();
  const loan = openLoan(l);
  l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'market', market: loan, when: 'defaulted' },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'lmsr', b: 10_000 },
      expiry: T0 + 40 * DAY,
    },
  });
  l.apply({ type: 'settle', by: WATCHER, market: loan, at: T0 + 31 * DAY });

  const replayed = Ledger.replay(l.log, { playground: true });
  assert.equal(replayed.digest(), l.digest(), 'a replay must land on the same digest');
  assert.deepEqual(replayed.snapshot(), l.snapshot());
  assert.notEqual(l.digest(), new Ledger({ playground: true }).digest());
});

test('a refused action leaves no trace, so ids stay reproducible', () => {
  // Offer ids derive from the action counter. If a refusal advanced it, every
  // id minted afterwards would differ from the one a replay produces, and the
  // log would stop being replayable at the first accept_offer. This is the
  // shape of a real failure: a live economy restarted and could not rebuild
  // itself, because an agent had been refused between declaring a market and
  // posting an offer on it.
  const l = world();
  const { market } = l.apply({ type: 'create_market', by: BORROWER, declaration: loanDeclaration(), at: T0 });

  const seqBefore = l.seq;
  const nowBefore = l.now;
  assert.throws(() => l.apply({
    type: 'transfer', by: BORROWER, to: LENDER, asset: 'USD', amount: 10 ** 12, at: T0 + 9 * DAY,
  }), /needs/);
  assert.equal(l.seq, seqBefore, 'a refusal must not advance the counter');
  assert.equal(l.now, nowBefore, 'nor the clock');

  const { offer } = l.apply({
    type: 'post_offer', by: BORROWER, market, leg: 'REPAID', stake: 150_000, ask: 100_000, at: T0,
  });
  l.apply({ type: 'accept_offer', by: LENDER, offer, at: T0 });

  const replayed = Ledger.replay(l.log, { playground: true });
  assert.equal(replayed.digest(), l.digest(), 'the log must replay across a refusal');
  assert.ok(replayed.offers.has(offer), 'the offer id must survive the replay');
});

test('taking an offer you cannot fully afford moves nothing', () => {
  // The ask was debited before the counter stake was checked, so an acceptor
  // who could pay one but not both ended up having paid the first while the
  // action was refused. Rolling back the counters cannot undo a transfer.
  const l = world(120_000);
  const { market } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'attestation', by: [ORACLE], quorum: 1 },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'bilateral' },
      expiry: T0 + DAY,
    },
  });
  const { offer } = l.apply({
    type: 'post_offer', by: BORROWER, market, leg: 'YES',
    stake: 10_000, ask: 100_000, counter_stake: 100_000, at: T0,
  });

  const before = { lender: l.balance(LENDER, 'USD'), borrower: l.balance(BORROWER, 'USD') };
  assert.throws(() => l.apply({ type: 'accept_offer', by: LENDER, offer, at: T0 }), /costs 200000/);
  assert.equal(l.balance(LENDER, 'USD'), before.lender, 'nothing left the acceptor');
  assert.equal(l.balance(BORROWER, 'USD'), before.borrower, 'and nothing reached the poster');
});

test('the clock never runs backwards', () => {
  const l = world();
  l.apply({ type: 'transfer', by: BORROWER, to: LENDER, asset: 'USD', amount: 1, at: T0 + 5 * DAY });
  l.apply({ type: 'transfer', by: BORROWER, to: LENDER, asset: 'USD', amount: 1, at: T0 });
  assert.equal(l.now, T0 + 5 * DAY, 'a slow clock cannot rewind the world');
});

// --- refusals ----------------------------------------------------------------

test('the obvious abuses are refused', () => {
  const l = world(200_000);
  const loan = openLoan(l);
  const m = l.markets.get(loan);

  const live = new Ledger({ playground: false });
  live.apply({ type: 'join', by: BORROWER, at: T0 });
  assert.throws(() => live.apply({ type: 'mint', by: BORROWER, asset: 'USD', amount: 1, at: T0 }),
    /playground only/);

  assert.throws(() => l.apply({ type: 'transfer', by: BORROWER, to: LENDER, asset: 'USD', amount: 10 ** 9, at: T0 }),
    /needs/);
  assert.throws(() => l.apply({ type: 'transfer', by: BORROWER, to: BORROWER, asset: 'USD', amount: 1, at: T0 }),
    /cannot pay yourself/);
  assert.throws(() => l.apply({ type: 'settle', by: WATCHER, market: loan, at: T0 + DAY }),
    /not ready to settle/);
  assert.throws(() => l.apply({ type: 'buy', by: WATCHER, market: loan, leg: 'REPAID', shares: 1, at: T0 }),
    /prices by lmsr|bilateral/);
  assert.throws(() => l.apply({ type: 'repay', by: WATCHER, market: loan, at: T0 + DAY }),
    /only a holder of the REPAID leg/);
  assert.equal(m.state, 'open');
});

test('an offer cannot be taken by its own poster and can be cancelled', () => {
  const l = world();
  const { market } = l.apply({ type: 'create_market', by: BORROWER, declaration: loanDeclaration(), at: T0 });
  const { offer } = l.apply({
    type: 'post_offer', by: BORROWER, market, leg: 'REPAID', stake: 150_000, ask: 100_000, at: T0,
  });
  assert.throws(() => l.apply({ type: 'accept_offer', by: BORROWER, offer, at: T0 }), /your own offer/);

  const before = l.balance(BORROWER, 'USD');
  l.apply({ type: 'cancel_offer', by: BORROWER, offer, at: T0 });
  assert.equal(l.balance(BORROWER, 'USD'), before + 150_000, 'cancelling returns the stake');
  assert.throws(() => l.apply({ type: 'accept_offer', by: LENDER, offer, at: T0 }), /offer is cancelled/);
});

test('a market nobody can decide expires and returns the stakes', () => {
  const l = world();
  const opening = total(l);
  const { market } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'attestation', by: [ORACLE], quorum: 1 },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'bilateral' },
      expiry: T0 + DAY,
    },
  });
  const { offer } = l.apply({
    type: 'post_offer', by: BORROWER, market, leg: 'YES', stake: 20_000, counter_stake: 20_000, at: T0,
  });
  l.apply({ type: 'accept_offer', by: LENDER, offer, at: T0 });

  const s = l.apply({ type: 'settle', by: WATCHER, market, at: T0 + 2 * DAY });
  assert.equal(s.state, 'expired');
  assert.equal(s.payouts[BORROWER], 20_000);
  assert.equal(s.payouts[LENDER], 20_000);
  assert.equal(total(l), opening);
});

// --- mechanism --------------------------------------------------------------

test('lmsr prices sum to one and move with the flow', () => {
  const p0 = prices([0, 0], 100);
  assert.ok(Math.abs(p0[0] + p0[1] - 1) < 1e-12);
  assert.ok(Math.abs(p0[0] - 0.5) < 1e-12, 'an untraded binary market is even money');

  const p1 = prices([500, 0], 100);
  assert.ok(p1[0] > 0.99, 'buying one side moves it toward certainty');

  // Buying more costs more per share as the price rises.
  const first = costToBuy([0, 0], 100, 0, 100);
  const later = costToBuy([400, 0], 100, 0, 100);
  assert.ok(later > first, 'the mechanism charges more as it becomes more certain');

  assert.equal(maxSubsidy(100, 2), Math.ceil(100 * Math.log(2)));
});

test('a bought share can be sold back for less than it cost', () => {
  const l = world();
  const { market } = l.apply({
    type: 'create_market', by: INSURER, at: T0,
    declaration: {
      collateral: { asset: 'USD' },
      positions: { kind: 'binary' },
      resolution: { kind: 'attestation', by: [ORACLE], quorum: 1 },
      payoff: { kind: 'winner_take_all' },
      mechanism: { kind: 'lmsr', b: 5_000 },
      expiry: T0 + DAY,
    },
  });
  const bought = l.apply({ type: 'buy', by: WATCHER, market, leg: 'YES', shares: 1_000, at: T0 });
  const sold = l.apply({ type: 'sell', by: WATCHER, market, leg: 'YES', shares: 1_000, at: T0 });
  assert.ok(sold.received <= bought.paid, 'round tripping cannot be profitable on its own');
  assert.ok(bought.paid - sold.received <= 2, 'and it costs no more than the rounding');
  assert.equal(quote(l.markets.get(market)).YES, 0.5);
});

// --- arithmetic --------------------------------------------------------------

test('pro rata conserves the total exactly', () => {
  for (const total of [1, 7, 100, 99_999, 1_000_000]) {
    for (const ws of [[1, 1, 1], [2, 3, 5], [1, 1], [7]]) {
      const weights = new Map(ws.map((w, i) => [`ag_${String(i).padStart(32, '0')}`, w]));
      const out = proRata(total, weights);
      const sum = [...out.values()].reduce((a, b) => a + b, 0);
      assert.equal(sum, total, `${total} across ${ws} must not leak`);
      assert.ok([...out.values()].every((v) => v >= 0));
    }
  }
  assert.equal([...proRata(10, new Map()).values()].length, 0);
});

test('median handles both parities', () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 1, 2, 3]), 2.5);
  assert.equal(median([]), null);
});
