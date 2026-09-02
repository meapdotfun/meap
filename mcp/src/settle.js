/**
 * Settlement: deciding what happened, then dividing the escrow.
 *
 * Split from the ledger because it is the part with no state of its own. Both
 * functions here are pure: given a market and a read-only view of the world
 * they return an answer, and the ledger is what actually moves balances. That
 * separation is what lets an agent ask "what would this pay me" without
 * touching anything, which it will want to do before taking a position.
 *
 * Every amount is an integer in minor units, and every division conserves the
 * total exactly. Escrow in equals escrow out, always; the remainder from an
 * uneven split is assigned deterministically rather than dropped, because a
 * ledger that loses a unit per settlement stops balancing after a few thousand
 * markets and no longer replays.
 */

// Taken from escrow at settlement, before any payout.
export const BOUNTY_BPS = 25;   // to whoever calls settle
export const ATTEST_BPS = 50;   // split among attestors who matched the outcome

const bps = (amount, n) => Math.floor((amount * n) / 10_000);

/**
 * Split `total` across `weights`, conserving `total` exactly.
 *
 * Largest-remainder: everyone gets their floor, and the units left over go to
 * the largest fractional parts. Ties break on address so that two agents
 * replaying the same log divide the same dust the same way.
 */
export function proRata(total, weights) {
  const entries = [...weights.entries()].filter(([, w]) => w > 0);
  const sum = entries.reduce((a, [, w]) => a + w, 0);
  const out = new Map();
  if (sum <= 0 || total <= 0) {
    for (const [addr] of entries) out.set(addr, 0);
    return out;
  }

  let assigned = 0;
  const rema = [];
  for (const [addr, w] of entries) {
    const exact = (total * w) / sum;
    const floor = Math.floor(exact);
    out.set(addr, floor);
    assigned += floor;
    rema.push([addr, exact - floor]);
  }

  rema.sort((a, b) => (b[1] - a[1]) || (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  for (let i = 0; assigned < total; i++, assigned++) {
    const addr = rema[i % rema.length][0];
    out.set(addr, out.get(addr) + 1);
  }
  return out;
}

/** Median of a numeric list. Even counts take the mean of the middle pair. */
export function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  if (!n) return null;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
}

// --- what happened ----------------------------------------------------------

/**
 * Decide whether a market can settle, and to what outcome.
 *
 * `view` supplies `now` and `market(id)`. Nothing here mutates.
 *
 * Returns { ready, outcome, state, why }. `state` is the terminal state the
 * market takes, which matters beyond this market: a market resolving on
 * another one reads exactly this field.
 */
export function resolve(market, view) {
  const d = market.declaration;
  const r = d.resolution;

  if (market.state !== 'open') {
    return { ready: false, why: `market is already ${market.state}` };
  }

  switch (r.kind) {
    case 'deadline': {
      // Only ever paired with a seizure payoff, so the question is simply
      // whether the obligation was discharged before the clock ran out.
      if (market.discharged) {
        const other = d.positions.legs.find((l) => l !== d.payoff.to);
        return { ready: true, outcome: { leg: other }, state: 'settled', why: 'discharged before the deadline' };
      }
      if (view.now >= r.at) {
        return { ready: true, outcome: { leg: d.payoff.to }, state: 'defaulted', why: 'deadline passed undischarged' };
      }
      return { ready: false, why: `deadline is ${r.at - view.now}ms away and nothing has been discharged` };
    }

    case 'attestation': {
      const reports = [...market.attestations.entries()].filter(([who]) => r.by.includes(who));
      if (reports.length < r.quorum) {
        return { ready: false, why: `${reports.length} of ${r.quorum} attestations in` };
      }
      if (d.positions.kind === 'scalar') {
        // Exact agreement on a number is not a reasonable ask, so the outcome
        // is the median of everything reported. One liar cannot move it.
        const value = median(reports.map(([, v]) => v.value));
        return { ready: true, outcome: { value }, state: 'settled', why: `median of ${reports.length} attestations` };
      }
      const tally = new Map();
      for (const [, v] of reports) tally.set(v.leg, (tally.get(v.leg) ?? 0) + 1);
      for (const [leg, n] of tally) {
        if (n >= r.quorum) {
          return { ready: true, outcome: { leg }, state: 'settled', why: `${n} attestors agreed on ${leg}` };
        }
      }
      return { ready: false, why: 'attestors reported but none reached quorum on one outcome' };
    }

    case 'market': {
      // The recursive case. Reading a market that has not settled yet is not
      // an error, it is simply not ready; this is how a chain of dependent
      // instruments unwinds in order.
      const ref = view.market(r.market);
      if (!ref) return { ready: false, why: `referenced market ${r.market} is unknown` };
      if (ref.state === 'open') return { ready: false, why: `referenced market has not settled` };

      const wanted = { resolved: 'settled', defaulted: 'defaulted', expired: 'expired' }[r.when];
      const held = ref.state === wanted;
      const [yes, no] = d.positions.legs;
      return {
        ready: true,
        outcome: { leg: held ? yes : no },
        state: 'settled',
        why: `referenced market is ${ref.state}, wanted ${wanted}`,
      };
    }

    default:
      return { ready: false, why: `unknown resolution kind ${r.kind}` };
  }
}

// --- who gets paid ----------------------------------------------------------

/** The fraction of escrow owed to the first leg, given a scalar outcome. */
function scalarFraction(d, value) {
  const { min, max } = d.positions;
  const v = Math.min(max, Math.max(min, value));
  if (d.payoff.kind === 'linear') return (v - min) / (max - min);
  const { strike, direction } = d.payoff;
  return direction === 'call'
    ? Math.max(0, v - strike) / (max - strike)
    : Math.max(0, strike - v) / (strike - min);
}

/**
 * Divide the escrow.
 *
 * `settler` is whoever called settle and earns the bounty. That it is a
 * parameter rather than the market's declarer is the design: closing out a
 * market that is ready is paid work open to anybody, so an agent can earn by
 * watching for obligations that have come due. Foreclosure is a job.
 *
 * Returns { payouts, bounty, attestors, dust } where payouts is a Map of
 * address to integer amount. The sum of everything returned equals the escrow.
 */
export function distribute(market, outcome, settler) {
  const d = market.declaration;
  const escrow = market.escrow;
  const payouts = new Map();
  const add = (addr, amt) => { if (amt > 0) payouts.set(addr, (payouts.get(addr) ?? 0) + amt); };

  const bounty = bps(escrow, BOUNTY_BPS);

  // Attestors who matched the outcome share a slice. Getting it wrong pays
  // nothing, which is the only thing making an attestation worth anything.
  let attestPool = 0;
  const paidAttestors = new Map();
  if (d.resolution.kind === 'attestation') {
    attestPool = bps(escrow, ATTEST_BPS);
    const r = d.resolution;
    const reports = [...market.attestations.entries()].filter(([who]) => r.by.includes(who));
    let matched;
    if (d.positions.kind === 'scalar') {
      // Within one percent of the range of the agreed value. An attestor who
      // reported an outlier moved nothing and earns nothing.
      const tol = (d.positions.max - d.positions.min) * 0.01;
      matched = reports.filter(([, v]) => Math.abs(v.value - outcome.value) <= tol);
    } else {
      matched = reports.filter(([, v]) => v.leg === outcome.leg);
    }
    for (const [who] of matched) paidAttestors.set(who, 1);
    if (!paidAttestors.size) attestPool = 0;
  }

  const net = escrow - bounty - attestPool;

  // Shares held on each leg.
  const held = (leg) => {
    const m = new Map();
    for (const [addr, legs] of market.holdings) {
      const q = legs.get(leg) ?? 0;
      if (q > 0) m.set(addr, q);
    }
    return m;
  };
  const pay = (total, weights) => {
    // Nobody holding the paying leg means the payoff describes no one. What is
    // left goes to the declarer, who is the residual claimant: they posted the
    // mechanism's subsidy and carried the risk that it would price badly.
    // Refunding the holders instead would hand the money back to the agents
    // who bet on the losing side, which is the one thing it must not do.
    const w = weights.size ? weights : new Map([[market.declarer, 1]]);
    for (const [addr, amt] of proRata(total, w)) add(addr, amt);
  };

  switch (d.payoff.kind) {
    case 'winner_take_all':
      pay(net, held(outcome.leg));
      break;

    case 'linear':
    case 'kinked': {
      const [long, short] = d.positions.legs;
      const f = scalarFraction(d, outcome.value);
      const toLong = Math.round(net * f);
      pay(toLong, held(long));
      pay(net - toLong, held(short));
      break;
    }

    case 'seizure': {
      // Discharged means the obligation was met: the escrowed collateral goes
      // back to the leg that posted it. The discharge payment itself already
      // moved to the other leg when it was paid, so it is not in escrow here.
      const other = d.positions.legs.find((l) => l !== d.payoff.to);
      pay(net, held(market.discharged ? other : d.payoff.to));
      break;
    }

    default:
      throw new Error(`unknown payoff kind ${d.payoff.kind}`);
  }

  let total = bounty + attestPool;
  for (const v of payouts.values()) total += v;
  const dust = escrow - total;

  return { payouts, bounty, bountyTo: settler, attestors: paidAttestors, attestPool, dust };
}
