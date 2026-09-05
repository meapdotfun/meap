/**
 * The ledger.
 *
 * Every verb an agent can call is a state transition applied here. Two
 * properties are load-bearing:
 *
 * Determinism. Nothing in this file reads a clock or a random number. Time
 * arrives as a field on the action and the ledger's own clock only moves
 * forward, so replaying an action log reproduces the state exactly, digest
 * included. That is what makes an economy of programs auditable: a participant
 * cannot claim a balance the log does not produce, and any agent can recompute
 * the whole history rather than trusting a report of it.
 *
 * Conservation. Value is only ever created by `mint`, which exists solely for
 * the stakes-off playground. Everywhere else, the sum of all balances plus all
 * escrows is invariant. `audit()` checks it, and the tests assert it after
 * every action of a full scenario, because a ledger that leaks a unit per
 * settlement stops balancing and stops replaying.
 *
 * Money is integer minor units. There is no floating point anywhere in a
 * balance; the only real arithmetic is inside LMSR pricing, which runs on the
 * deterministic exp and log in math.js and rounds to integers on the way out.
 */

import { Digest } from './digest.js';
import { validate, marketId, canonical, checkResolutionGraph, Bad } from './grammar.js';
import { costToBuy, proceedsFromSell, prices, maxSubsidy } from './lmsr.js';
import { resolve, distribute, proRata } from './settle.js';

export class Refused extends Error {
  constructor(msg) { super(msg); this.name = 'Refused'; }
}

const need = (cond, msg) => { if (!cond) throw new Refused(msg); };
const isInt = (x) => Number.isInteger(x);
const isAmt = (x) => Number.isInteger(x) && x > 0 && x <= Number.MAX_SAFE_INTEGER;

/**
 * A stable address for a label.
 *
 * Whatever the label is, is the account: a shared deployment passes a bearer
 * secret so that guessing the label is as hard as guessing the secret. That is
 * still weaker than a key, because whoever receives it can use it.
 */
export function addressOf(seed) {
  return `ag_${new Digest().text(`agent:${seed}`).hex()}`;
}

/**
 * An address derived from an ed25519 public key.
 *
 * Separately namespaced from `addressOf` so a key and a label can never land
 * on the same address, which would let the weaker scheme spend the stronger
 * one's balance.
 */
export function addressOfKey(publicKeyHex) {
  return `ag_${new Digest().text(`key:${String(publicKeyHex).toLowerCase()}`).hex()}`;
}

/**
 * An address no credential can authenticate as.
 *
 * The treasury was originally derived with addressOf, which is the same
 * function bearer tokens flow through, so presenting the treasury's label AS a
 * token made you the treasury: the entire pot was spendable by anyone who read
 * the source. Repo-readable string, live money. This namespace exists so that
 * system accounts live where neither a bearer token (agent:) nor a public key
 * (key:) can ever land.
 */
export function addressOfSystem(name) {
  return `ag_${new Digest().text(`system:${name}`).hex()}`;
}

export class Ledger {
  /**
   * @param {object} opts
   * @param {boolean} opts.playground  allow `mint`, i.e. run with consequences
   *   off. The same verb set either way; only the funding differs.
   * @param {object|null} opts.opening  {asset, amount} credited once when an
   *   address first joins. This is how a shared economy funds arrivals without
   *   opening `mint` to everyone: a grant fixed at join is equal for all,
   *   unrepeatable, and replayed from the log like anything else.
   * @param {object|null} opts.treasury  {address, asset, amount, taper}
   *   holding the entire supply at genesis. Grants are paid out of it rather
   *   than minted, which is what stops registration being a money press: an
   *   attacker who registers ten thousand addresses drains a fixed pot instead
   *   of creating ten billion from nothing. `taper` bounds each grant to
   *   held/taper, so the faucet also cannot be emptied outright: harvesting
   *   gets exponentially less rewarding as the pot shrinks, and registration
   *   floods decay instead of racing to zero. The address must come from
   *   addressOfSystem, never addressOf, or the pot is spendable by whoever
   *   presents its label as a bearer token.
   */
  constructor({ playground = false, opening = null, treasury = null } = {}) {
    this.playground = playground;
    this.opening = opening;
    this.treasury = treasury;
    this.agents = new Map();
    this.markets = new Map();
    this.offers = new Map();
    this.log = [];
    this.seq = 0;
    this.now = 0;

    // Genesis. Not an action, because nothing happened: this is the supply
    // existing. It is part of the configuration, so a replay reconstructs it
    // before touching the log and lands on the same digest.
    if (treasury) {
      this.agents.set(treasury.address, {
        address: treasury.address,
        balances: new Map([[treasury.asset, treasury.amount]]),
        joined: 0,
        stats: { declared: 0, positions: 0, settled: 0, defaults: 0, attestations: 0, forecloses: 0 },
      });
    }
  }

  // --- reads ----------------------------------------------------------------

  account(addr) {
    const a = this.agents.get(addr);
    need(a, `unknown agent ${addr}`);
    return a;
  }

  balance(addr, asset) {
    return this.agents.get(addr)?.balances.get(asset) ?? 0;
  }

  market(id) {
    const m = this.markets.get(id);
    need(m, `unknown market ${id}`);
    return m;
  }

  /** Read-only handle handed to the pure settlement functions. */
  view() {
    return { now: this.now, market: (id) => this.markets.get(id) ?? null };
  }

  /** Total held everywhere. Must not change except across a mint. */
  audit() {
    const totals = new Map();
    const bump = (asset, n) => totals.set(asset, (totals.get(asset) ?? 0) + n);
    for (const a of this.agents.values()) for (const [asset, n] of a.balances) bump(asset, n);
    for (const m of this.markets.values()) bump(m.asset, m.escrow);
    return totals;
  }

  digest() {
    return new Digest().text(canonical(this.snapshot())).hex();
  }

  snapshot() {
    const mapObj = (m, f = (v) => v) => Object.fromEntries([...m].sort(cmp0).map(([k, v]) => [k, f(v)]));
    return {
      seq: this.seq,
      now: this.now,
      agents: mapObj(this.agents, (a) => ({
        address: a.address,
        balances: Object.fromEntries([...a.balances].sort(cmp0)),
        joined: a.joined,
        stats: { ...a.stats },
      })),
      markets: mapObj(this.markets, (m) => ({
        id: m.id,
        declarer: m.declarer,
        declaration: m.declaration,
        state: m.state,
        asset: m.asset,
        escrow: m.escrow,
        q: [...m.q],
        discharged: m.discharged,
        holdings: Object.fromEntries(
          [...m.holdings].sort(cmp0).map(([addr, legs]) => [addr, Object.fromEntries([...legs].sort(cmp0))]),
        ),
        attestations: Object.fromEntries([...m.attestations].sort(cmp0)),
        outcome: m.outcome,
      })),
      offers: mapObj(this.offers, (o) => ({ ...o })),
    };
  }

  // --- writes ---------------------------------------------------------------

  /**
   * Apply one action.
   *
   * Actions carry their own timestamp. The ledger's clock takes the maximum of
   * what it has seen, so an agent with a slow clock cannot rewind the world and
   * the ordering stays whatever the log says it was.
   */
  apply(action) {
    const { type, by, at } = action;
    need(typeof type === 'string', 'action needs a type');
    need(typeof by === 'string', 'action needs a `by` address');
    need(isInt(at) && at > 0, 'action needs a millisecond timestamp `at`');

    const fn = HANDLERS[type];
    need(fn, `unknown action ${type}`);
    if (type !== 'join') this.account(by);

    // The clock and the counter advance for the handler to see, then roll back
    // if it refuses. A refused action is not logged, so anything it moved would
    // be invisible to a replay: the counter feeds offer ids, and a refusal
    // between two accepted actions would otherwise shift every id after it and
    // make the log unreplayable. Found by restarting a live economy and
    // watching accept_offer fail against an id that no longer existed.
    const wasNow = this.now;
    const wasSeq = this.seq;
    this.now = Math.max(this.now, at);
    this.seq += 1;
    let result;
    try {
      result = fn.call(this, action);
    } catch (e) {
      this.now = wasNow;
      this.seq = wasSeq;
      throw e;
    }
    this.log.push(action);
    return { seq: this.seq, ...result };
  }

  /**
   * Rebuild from an action log. Must reproduce the digest exactly.
   *
   * `opts` has to match the ledger the log came from, `opening` included: the
   * grant is applied by the join handler, so replaying with a different grant
   * yields a different and equally self consistent economy.
   */
  static replay(log, opts = {}) {
    const l = new Ledger(opts);
    for (const a of log) l.apply(a);
    return l;
  }

  // --- internals ------------------------------------------------------------

  _credit(addr, asset, amount) {
    const a = this.account(addr);
    a.balances.set(asset, (a.balances.get(asset) ?? 0) + amount);
  }

  _debit(addr, asset, amount) {
    const a = this.account(addr);
    const have = a.balances.get(asset) ?? 0;
    need(have >= amount, `${addr} holds ${have} ${asset}, needs ${amount}`);
    a.balances.set(asset, have - amount);
  }

  _grantShares(m, addr, leg, shares) {
    if (!m.holdings.has(addr)) m.holdings.set(addr, new Map());
    const legs = m.holdings.get(addr);
    legs.set(leg, (legs.get(leg) ?? 0) + shares);
  }

  _legIndex(m, leg) {
    const i = m.declaration.positions.legs.indexOf(leg);
    need(i >= 0, `"${leg}" is not a leg of this market; legs are ${m.declaration.positions.legs.join(', ')}`);
    return i;
  }
}

const cmp0 = (a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0);

// --- the verbs --------------------------------------------------------------

const HANDLERS = {

  /** Register. An address that has never acted does not exist. */
  join(a) {
    if (this.agents.has(a.by)) return { address: a.by, existing: true };
    this.agents.set(a.by, {
      address: a.by,
      balances: new Map(),
      joined: this.seq,
      stats: { declared: 0, positions: 0, settled: 0, defaults: 0, attestations: 0, forecloses: 0 },
    });
    // Granted inside `join` rather than as a verb of its own, so it cannot be
    // called twice: joining is idempotent and this rides along with it.
    let granted = null;
    if (this.opening) {
      const { asset, amount } = this.opening;
      if (this.treasury) {
        // Paid, not printed, and tapered: a grant is at most 1/taper of what
        // remains, so a sybil flood harvests a decaying pot rather than
        // emptying it, and when it runs low an arrival gets little and has to
        // be paid by somebody who already holds some. Deterministic from
        // ledger state alone, so replay needs no record of the policy.
        const pot = this.agents.get(this.treasury.address);
        const have = pot.balances.get(asset) ?? 0;
        const give = Math.min(amount, Math.floor(have / (this.treasury.taper || 1)));
        if (give > 0) {
          pot.balances.set(asset, have - give);
          this._credit(a.by, asset, give);
          granted = { asset, amount: give };
        }
      } else {
        this._credit(a.by, asset, amount);
        granted = { asset, amount };
      }
    }
    return { address: a.by, existing: false, granted };
  },

  /**
   * Create value. Playground only.
   *
   * This is the single difference between the playground and a live chain: the
   * verb set is identical, and only the question of where the money comes from
   * changes. On a chain this handler does not exist and balances arrive by
   * deposit.
   */
  mint(a) {
    need(this.playground, 'mint is playground only; on a live ledger balances arrive by deposit');
    need(isAmt(a.amount), 'amount must be a positive integer');
    this._credit(a.by, a.asset, a.amount);
    return { balance: this.balance(a.by, a.asset) };
  },

  /**
   * Pay another agent. Also how an agent hires another: the memo says what for
   * and the payment is the whole contract.
   */
  transfer(a) {
    need(isAmt(a.amount), 'amount must be a positive integer');
    this.account(a.to);
    need(a.to !== a.by, 'cannot pay yourself');
    this._debit(a.by, a.asset, a.amount);
    this._credit(a.to, a.asset, a.amount);
    return { to: a.to, amount: a.amount, balance: this.balance(a.by, a.asset) };
  },

  /**
   * Declare a market.
   *
   * An LMSR market has to be seeded by its declarer with the mechanism's
   * bounded subsidy, because a scoring rule quotes prices with no counterparty
   * and something has to stand behind that. The amount is exactly
   * `maxSubsidy`, which the declaration itself makes computable in advance, so
   * an agent knows the cost of declaring before it declares.
   */
  create_market(a) {
    const decl = validate(a.declaration, { now: this.now });
    checkResolutionGraph(decl, (id) => this.markets.get(id) ?? null);

    const id = marketId(decl, a.by);
    need(!this.markets.has(id), 'that exact market already exists; vary the label or the terms');

    let seed = 0;
    if (decl.mechanism.kind === 'lmsr') {
      seed = maxSubsidy(decl.mechanism.b, decl.positions.legs.length);
      this._debit(a.by, decl.collateral.asset, seed);
    }

    this.markets.set(id, {
      id,
      declarer: a.by,
      declaration: decl,
      state: 'open',
      asset: decl.collateral.asset,
      escrow: seed,
      q: decl.positions.legs.map(() => 0),
      holdings: new Map(),
      attestations: new Map(),
      discharged: false,
      outcome: null,
      createdAt: this.now,
    });
    this.account(a.by).stats.declared += 1;
    return { market: id, seeded: seed, legs: decl.positions.legs };
  },

  /**
   * Offer one side of a bilateral market.
   *
   * `stake` is escrowed on posting, so an offer is always backed. `ask` is what
   * the poster wants paid directly by whoever takes the other side, and it is
   * what makes a loan expressible: the borrower stakes collateral, asks for the
   * principal, and requires nothing of the lender beyond paying it.
   */
  post_offer(a) {
    const m = this.market(a.market);
    need(m.state === 'open', `market is ${m.state}`);
    need(m.declaration.mechanism.kind === 'bilateral', 'this market prices by lmsr; use buy');
    this._legIndex(m, a.leg);

    const stake = a.stake ?? 0;
    const ask = a.ask ?? 0;
    const counter = a.counter_stake ?? 0;
    need(isInt(stake) && stake >= 0, 'stake must be a non-negative integer');
    need(isInt(ask) && ask >= 0, 'ask must be a non-negative integer');
    need(isInt(counter) && counter >= 0, 'counter_stake must be a non-negative integer');
    need(stake + counter > 0, 'an offer with nothing at stake on either side settles nothing');
    need(stake + counter >= m.declaration.collateral.min, `this market requires at least ${m.declaration.collateral.min} at stake`);

    if (stake > 0) {
      this._debit(a.by, m.asset, stake);
      m.escrow += stake;
    }

    const id = `of_${new Digest().text(`${a.market}:${a.by}:${this.seq}`).hex()}`;
    this.offers.set(id, {
      id, market: a.market, from: a.by, leg: a.leg, stake, ask, counterStake: counter,
      state: 'open', createdAt: this.now,
    });
    return { offer: id, escrowed: stake };
  },

  cancel_offer(a) {
    const o = this.offers.get(a.offer);
    need(o, `unknown offer ${a.offer}`);
    need(o.from === a.by, 'only the poster can cancel an offer');
    need(o.state === 'open', `offer is ${o.state}`);
    const m = this.market(o.market);
    if (o.stake > 0) { m.escrow -= o.stake; this._credit(a.by, m.asset, o.stake); }
    o.state = 'cancelled';
    return { offer: o.id, returned: o.stake };
  },

  /**
   * Take the other side.
   *
   * Both parties receive shares equal to the pair's total contribution to
   * escrow, not to their own. That is what lets a lender who escrows nothing
   * still hold the full claim on the collateral: the claim is on the pot, and
   * both sides are claiming the same pot from opposite directions.
   */
  accept_offer(a) {
    const o = this.offers.get(a.offer);
    need(o, `unknown offer ${a.offer}`);
    need(o.state === 'open', `offer is ${o.state}`);
    need(o.from !== a.by, 'cannot take your own offer');

    const m = this.market(o.market);
    need(m.state === 'open', `market is ${m.state}`);
    const legs = m.declaration.positions.legs;
    const other = legs.find((l) => l !== o.leg);
    need(other, 'a bilateral offer needs a market with two legs');

    // Checked as one sum before anything moves. Debiting the ask first and the
    // counter stake second would, for an acceptor who can afford one but not
    // both, leave the ask already transferred when the second debit threw.
    // Rolling the counters back does not undo a transfer, so the guard has to
    // come first.
    const owed = o.ask + o.counterStake;
    const have = this.account(a.by).balances.get(m.asset) ?? 0;
    need(have >= owed, `taking this offer costs ${owed} ${m.asset} and you hold ${have}`);

    if (o.ask > 0) { this._debit(a.by, m.asset, o.ask); this._credit(o.from, m.asset, o.ask); }
    if (o.counterStake > 0) { this._debit(a.by, m.asset, o.counterStake); m.escrow += o.counterStake; }

    const shares = o.stake + o.counterStake;
    this._grantShares(m, o.from, o.leg, shares);
    this._grantShares(m, a.by, other, shares);
    this.account(o.from).stats.positions += 1;
    this.account(a.by).stats.positions += 1;

    o.state = 'taken';
    o.takenBy = a.by;
    return { offer: o.id, market: m.id, youHold: { leg: other, shares }, theyHold: { leg: o.leg, shares }, paid: o.ask + o.counterStake };
  },

  /** Buy shares from a scoring rule. Cost is quoted by the mechanism. */
  buy(a) {
    const m = this.market(a.market);
    need(m.state === 'open', `market is ${m.state}`);
    need(m.declaration.mechanism.kind === 'lmsr', 'this market is bilateral; post or accept an offer');
    need(isAmt(a.shares), 'shares must be a positive integer');
    const i = this._legIndex(m, a.leg);
    const b = m.declaration.mechanism.b;

    const price = costToBuy(m.q, b, i, a.shares);
    need(price >= m.declaration.collateral.min, `this market requires at least ${m.declaration.collateral.min} at stake`);
    this._debit(a.by, m.asset, price);
    m.escrow += price;
    m.q[i] += a.shares;
    this._grantShares(m, a.by, a.leg, a.shares);
    this.account(a.by).stats.positions += 1;

    return { market: m.id, leg: a.leg, shares: a.shares, paid: price, prices: quote(m) };
  },

  /** Sell shares back. Proceeds are quoted the same way. */
  sell(a) {
    const m = this.market(a.market);
    need(m.state === 'open', `market is ${m.state}`);
    need(m.declaration.mechanism.kind === 'lmsr', 'this market is bilateral');
    need(isAmt(a.shares), 'shares must be a positive integer');
    const i = this._legIndex(m, a.leg);
    const legs = this.market(a.market).holdings.get(a.by);
    need((legs?.get(a.leg) ?? 0) >= a.shares, 'you do not hold that many shares');

    const proceeds = proceedsFromSell(m.q, m.declaration.mechanism.b, i, a.shares);
    m.escrow -= proceeds;
    m.q[i] -= a.shares;
    legs.set(a.leg, legs.get(a.leg) - a.shares);
    this._credit(a.by, m.asset, proceeds);
    return { market: m.id, leg: a.leg, shares: a.shares, received: proceeds, prices: quote(m) };
  },

  /**
   * Report what happened.
   *
   * Only a named attestor may report, and only once. Settlement pays the
   * attestors who matched the outcome and nothing to the rest, so this is work
   * with a wage and a way to be caught, which is all an oracle ever was.
   */
  attest(a) {
    const m = this.market(a.market);
    need(m.state === 'open', `market is ${m.state}`);
    const r = m.declaration.resolution;
    need(r.kind === 'attestation', 'this market does not resolve by attestation');
    need(r.by.includes(a.by), 'you are not an attestor on this market');
    need(!m.attestations.has(a.by), 'you have already attested; reports are final');

    if (m.declaration.positions.kind === 'scalar') {
      need(typeof a.value === 'number' && Number.isFinite(a.value), 'a scalar market needs a numeric `value`');
      m.attestations.set(a.by, { value: a.value });
    } else {
      this._legIndex(m, a.leg);
      m.attestations.set(a.by, { leg: a.leg });
    }
    this.account(a.by).stats.attestations += 1;
    return { market: m.id, reports: m.attestations.size, quorum: r.quorum };
  },

  /**
   * Discharge an obligation before it is seized.
   *
   * The payment goes straight to the holders of the seizing leg, in proportion
   * to their claim; the escrowed collateral stays put and returns to the payer
   * at settlement. Anyone holding the obliged leg may pay, which means a third
   * party can rescue a position it did not open.
   */
  repay(a) {
    const m = this.market(a.market);
    need(m.state === 'open', `market is ${m.state}`);
    const d = m.declaration;
    need(d.payoff.kind === 'seizure', 'nothing to discharge: this market has no seizure payoff');
    need(!m.discharged, 'already discharged');
    need(this.now < d.resolution.at, 'the deadline has passed; this can only be foreclosed now');

    const obliged = d.positions.legs.find((l) => l !== d.payoff.to);
    need((m.holdings.get(a.by)?.get(obliged) ?? 0) > 0, `only a holder of the ${obliged} leg can discharge this`);

    const amount = d.payoff.discharge;
    need(amount > 0, 'this market declares no discharge amount, so it can only be seized');
    this._debit(a.by, m.asset, amount);

    const claims = new Map();
    for (const [addr, legs] of m.holdings) {
      const q = legs.get(d.payoff.to) ?? 0;
      if (q > 0) claims.set(addr, q);
    }
    need(claims.size, 'nobody holds the seizing leg yet');
    for (const [addr, amt] of proRata(amount, claims)) this._credit(addr, m.asset, amt);

    m.discharged = true;
    return { market: m.id, paid: amount, to: [...claims.keys()] };
  },

  /**
   * Close out a market that is ready, and take the bounty for doing it.
   *
   * Open to anybody, which is the point. An agent can earn by watching for
   * obligations that have come due and acting on them, so foreclosure is a job
   * rather than a privilege of the lender.
   */
  settle(a) {
    const m = this.market(a.market);
    need(m.state === 'open', `market is already ${m.state}`);

    const r = resolve(m, this.view());
    if (!r.ready) {
      // Nothing can decide it and the window has closed: unwind at par.
      if (this.now >= m.declaration.expiry) {
        const back = new Map();
        for (const [addr, legs] of m.holdings) {
          let t = 0;
          for (const q of legs.values()) t += q;
          if (t > 0) back.set(addr, t);
        }
        const payouts = back.size ? proRata(m.escrow, back) : new Map([[m.declarer, m.escrow]]);
        for (const [addr, amt] of payouts) this._credit(addr, m.asset, amt);
        m.escrow = 0;
        m.state = 'expired';
        return { market: m.id, state: 'expired', why: 'expired undecided; stakes returned', payouts: obj(payouts) };
      }
      throw new Refused(`not ready to settle: ${r.why}`);
    }

    const dist = distribute(m, r.outcome, a.by);
    for (const [addr, amt] of dist.payouts) this._credit(addr, m.asset, amt);
    if (dist.bounty > 0) this._credit(a.by, m.asset, dist.bounty);
    if (dist.attestPool > 0) {
      for (const [who, amt] of proRata(dist.attestPool, dist.attestors)) this._credit(who, m.asset, amt);
    }
    // Rounding dust cannot stay in a settled market or the ledger stops
    // balancing. It goes to whoever did the work of closing it.
    if (dist.dust > 0) this._credit(a.by, m.asset, dist.dust);

    m.escrow = 0;
    m.state = r.state;
    m.outcome = r.outcome;
    this.account(a.by).stats.forecloses += 1;
    for (const addr of m.holdings.keys()) {
      const acct = this.agents.get(addr);
      if (acct) acct.stats.settled += 1;
    }
    if (r.state === 'defaulted') {
      const obliged = m.declaration.positions.legs.find((l) => l !== m.declaration.payoff.to);
      for (const [addr, legs] of m.holdings) {
        if ((legs.get(obliged) ?? 0) > 0) this.agents.get(addr).stats.defaults += 1;
      }
    }

    return {
      market: m.id, state: r.state, outcome: r.outcome, why: r.why,
      payouts: obj(dist.payouts), bounty: dist.bounty + dist.dust, attestPool: dist.attestPool,
    };
  },
};

const obj = (m) => Object.fromEntries([...m].sort(cmp0));

/** Current prices for an lmsr market, keyed by leg. */
export function quote(m) {
  if (m.declaration.mechanism.kind !== 'lmsr') return null;
  const p = prices(m.q, m.declaration.mechanism.b);
  return Object.fromEntries(m.declaration.positions.legs.map((l, i) => [l, Number(p[i].toFixed(6))]));
}

export { Bad };
