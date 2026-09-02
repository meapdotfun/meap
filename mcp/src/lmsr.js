/**
 * Logarithmic market scoring rule (Hanson, 2003).
 *
 *   C(q) = b * ln( sum_i exp(q_i / b) )
 *   p_i  = exp(q_i / b) / sum_j exp(q_j / b)
 *
 * The cost of moving the share vector from q to q' is C(q') - C(q). Prices are
 * always positive and always sum to one, so the market quotes a price with no
 * counterparty present. That matters here more than it does for humans: an
 * agent acting at four in the morning has nobody to trade against, and a
 * mechanism that requires a resting order book would simply not fill.
 *
 * The subsidy the declarer implicitly provides is bounded by b*ln(n) across
 * the market's whole life, where n is the number of legs. That bound is why
 * b is a required parameter rather than a tuning knob: it is the maximum the
 * mechanism can lose.
 *
 * Everything runs through dexp/dlog rather than Math.exp/Math.log. ECMAScript
 * leaves those implementation-approximated, and two agents replaying the same
 * action log on different engines would otherwise compute different balances
 * from the same history.
 *
 * Money is integer minor units throughout. Costs round away from the trader,
 * so escrow can never end a trade short by a rounding error.
 */

import { dexp, dlog } from './math.js';

/**
 * C(q) in minor units, as a real number.
 *
 * Computed with the max subtracted out. q_i/b reaches into the hundreds for a
 * market that has traded heavily, and exp of that overflows to Infinity long
 * before the arithmetic is meaningless.
 */
export function cost(q, b) {
  let m = -Infinity;
  for (const x of q) if (x > m) m = x;
  let sum = 0;
  for (const x of q) sum += dexp((x - m) / b);
  return m + b * dlog(sum);
}

/** Instantaneous prices, one per leg. Sums to 1. */
export function prices(q, b) {
  let m = -Infinity;
  for (const x of q) if (x > m) m = x;
  const e = q.map((x) => dexp((x - m) / b));
  let sum = 0;
  for (const v of e) sum += v;
  return e.map((v) => v / sum);
}

/**
 * Integer cost of buying `shares` of leg `i`, rounded up.
 *
 * Rounding up rather than to nearest is deliberate. The trader pays the
 * fraction of a minor unit rather than the escrow absorbing it, which keeps
 * the invariant that escrow always covers the maximum possible payout.
 */
export function costToBuy(q, b, i, shares) {
  if (!Number.isInteger(shares) || shares <= 0) throw new Error('shares must be a positive integer');
  const before = cost(q, b);
  const after = cost(q.map((x, j) => (j === i ? x + shares : x)), b);
  return Math.ceil(after - before);
}

/**
 * Integer proceeds from selling `shares` of leg `i`, rounded down.
 *
 * Rounded down for the same reason buying rounds up: the fractional unit stays
 * with the escrow rather than leaving it.
 */
export function proceedsFromSell(q, b, i, shares) {
  if (!Number.isInteger(shares) || shares <= 0) throw new Error('shares must be a positive integer');
  if (q[i] - shares < 0) throw new Error('cannot sell more shares than exist on that leg');
  const before = cost(q, b);
  const after = cost(q.map((x, j) => (j === i ? x - shares : x)), b);
  return Math.floor(before - after);
}

/**
 * The most this mechanism can pay out beyond what it took in.
 *
 * A declarer can read this before committing, which is the whole point of the
 * declaration being inspectable.
 */
export function maxSubsidy(b, legs) {
  return Math.ceil(b * dlog(legs));
}
