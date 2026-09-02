/**
 * The market grammar.
 *
 * A market is declared as data, never as code. The engine interprets the
 * declaration; agents never deploy logic. That choice is the load-bearing one
 * in this design, and it is not primarily about safety:
 *
 *   - A declaration can be read by the agent on the other side. The single
 *     most useful property of an economy whose participants are programs is
 *     that your counterparty is inspectable. Arbitrary bytecode destroys it.
 *   - A declaration can be indexed. Every instrument that exists can be
 *     enumerated, displayed and watched, which is what makes the economy
 *     legible from outside.
 *   - The untrusted part is isolated. `label` is free text written by one
 *     agent and read by another, so it is an injection surface. Everything
 *     else in a declaration is a closed vocabulary that can be evaluated
 *     without reading prose.
 *
 * The vocabulary is small. It stays unbounded because a market may resolve on
 * another market's outcome, so declarations compose: insurance on a loan, a
 * market on whether that insurance pays, and so on. Depth is capped only by
 * the cycle check in `validate`.
 *
 * Every conventional instrument falls out of the five fields rather than being
 * a special case in the engine:
 *
 *   loan       deadline resolution + seizure payoff
 *   option     scalar positions + kinked payoff
 *   insurance  resolution on another market's default
 *   prediction the base case
 */

import { Digest } from './digest.js';

// --- vocabulary -------------------------------------------------------------

/** What can be held. */
const POSITIONS = {
  binary: (p) => {
    if (Object.keys(p).length !== 1) throw new Bad('positions.binary takes no arguments');
    return { kind: 'binary', legs: ['YES', 'NO'] };
  },
  categorical: (p) => {
    const o = p.outcomes;
    if (!Array.isArray(o) || o.length < 2 || o.length > 32) {
      throw new Bad('positions.categorical needs 2 to 32 outcomes');
    }
    if (o.some((x) => typeof x !== 'string' || !x)) throw new Bad('outcomes must be non-empty strings');
    if (new Set(o).size !== o.length) throw new Bad('outcomes must be distinct');
    return { kind: 'categorical', legs: [...o] };
  },
  scalar: (p) => {
    const { min, max } = p;
    if (!finite(min) || !finite(max)) throw new Bad('positions.scalar needs finite min and max');
    if (!(max > min)) throw new Bad('positions.scalar needs max > min');
    return { kind: 'scalar', min, max, legs: ['LONG', 'SHORT'] };
  },
};

/**
 * Where truth arrives from.
 *
 * There is no `oracle` kind, and its absence is a claim rather than an
 * omission. An oracle is an agent that reports a number and is trusted because
 * it has been right before. That is exactly what `attestation` already is, so
 * a separate kind would only add a privileged class of reporter the grammar
 * cannot justify. In an economy whose participants are all programs, truth
 * telling is a job someone is paid for and can be caught not doing, and
 * settlement pays the attestors who matched the outcome.
 *
 * `market` is the recursive case and the reason the instrument space is not
 * enumerable in advance. It reads another market's settled outcome as this
 * market's input.
 */
const RESOLUTION = {
  deadline: (r) => {
    if (!ts(r.at)) throw new Bad('resolution.deadline needs a millisecond timestamp `at`');
    return { kind: 'deadline', at: r.at };
  },
  attestation: (r) => {
    const by = r.by;
    if (!Array.isArray(by) || by.length < 1 || by.length > 16) {
      throw new Bad('resolution.attestation needs 1 to 16 attestors');
    }
    if (by.some((a) => !addr(a))) throw new Bad('attestors must be agent addresses');
    if (new Set(by).size !== by.length) throw new Bad('attestors must be distinct');
    const quorum = r.quorum ?? by.length;
    if (!Number.isInteger(quorum) || quorum < 1 || quorum > by.length) {
      throw new Bad('quorum must be an integer in 1..by.length');
    }
    return { kind: 'attestation', by: [...by], quorum };
  },
  market: (r) => {
    if (!id(r.market)) throw new Bad('resolution.market needs a market id');
    const when = r.when ?? 'resolved';
    if (!['resolved', 'defaulted', 'expired'].includes(when)) {
      throw new Bad("resolution.market `when` must be resolved, defaulted or expired");
    }
    return { kind: 'market', market: r.market, when };
  },
};

/**
 * How escrowed collateral maps onto positions at settlement.
 *
 * Note what cannot be expressed: a destination. Payoffs describe proportions
 * across the declaration's own legs, so a market has no way to name an address
 * that drains it. Collateral is held by the engine and can only ever flow back
 * to position holders. That is why a declaring agent cannot write a market
 * that steals its own backing.
 */
const PAYOFF = {
  winner_take_all: () => ({ kind: 'winner_take_all' }),
  linear: () => ({ kind: 'linear' }),
  kinked: (p) => {
    if (!finite(p.strike)) throw new Bad('payoff.kinked needs a finite strike');
    if (!['call', 'put'].includes(p.direction)) throw new Bad("payoff.kinked direction must be call or put");
    return { kind: 'kinked', strike: p.strike, direction: p.direction };
  },
  seizure: (p) => {
    const to = p.to ?? 'YES';
    // The amount the opposing leg may pay to settle in its own favour before
    // resolution. This is what makes `repay` expressible: without it a loan
    // could only ever default, because nothing in the grammar would let the
    // borrower discharge the obligation.
    const discharge = p.discharge ?? 0;
    if (!finite(discharge) || discharge < 0) {
      throw new Bad('payoff.seizure discharge must be a non-negative number');
    }
    return { kind: 'seizure', to: String(to), discharge };
  },
};

/**
 * How positions change hands before settlement.
 *
 * Two, both fully implemented. An order book and a sealed auction both belong
 * here eventually, but a vocabulary entry the engine cannot execute is a lie
 * told to every agent that reads it, so they are absent rather than stubbed.
 */
const MECHANISM = {
  // Peer to peer. One agent posts terms, another takes the other side. This is
  // the shape most agent to agent credit takes: there is no book to rest on.
  bilateral: () => ({ kind: 'bilateral' }),

  // Logarithmic market scoring rule. Always quotes a price, so an agent can
  // trade against a market with no counterparty present, at a bounded and
  // known subsidy of b*ln(n).
  lmsr: (m) => {
    const b = m.b ?? 100;
    if (!finite(b) || b <= 0) throw new Bad('mechanism.lmsr needs a positive liquidity parameter b');
    return { kind: 'lmsr', b };
  },
};

// --- validation -------------------------------------------------------------

export class Bad extends Error {
  constructor(msg) { super(msg); this.name = 'BadDeclaration'; }
}

const finite = (x) => typeof x === 'number' && Number.isFinite(x);
const ts = (x) => Number.isInteger(x) && x > 0 && x < 4e15;
const addr = (x) => typeof x === 'string' && /^ag_[0-9a-f]{32}$/.test(x);
const id = (x) => typeof x === 'string' && /^mk_[0-9a-f]{32}$/.test(x);

/** Dispatch one field through its vocabulary table. */
function pick(table, field, value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Bad(`${field} must be an object`);
  }
  const fn = table[value.kind];
  if (!fn) {
    throw new Bad(`${field}.kind must be one of: ${Object.keys(table).join(', ')}`);
  }
  return fn(value);
}

/**
 * Combinations the vocabulary allows individually but that do not mean
 * anything together. Checked explicitly so a nonsense market is rejected at
 * declaration rather than discovered at settlement.
 */
function coherent(d) {
  const pk = d.positions.kind;
  const py = d.payoff.kind;

  if (py === 'linear' && pk !== 'scalar') throw new Bad('payoff.linear requires scalar positions');
  if (py === 'kinked' && pk !== 'scalar') throw new Bad('payoff.kinked requires scalar positions');
  if (py === 'winner_take_all' && pk === 'scalar') {
    throw new Bad('payoff.winner_take_all requires binary or categorical positions');
  }
  if (py === 'seizure') {
    if (d.positions.legs.length !== 2) {
      throw new Bad('payoff.seizure needs exactly two legs, so that discharge has one payer');
    }
    if (!d.positions.legs.includes(d.payoff.to)) {
      throw new Bad(`payoff.seizure to "${d.payoff.to}" is not one of this market's legs`);
    }
  }
  if (d.payoff.kind === 'kinked') {
    const { min, max } = d.positions;
    if (d.payoff.strike < min || d.payoff.strike > max) {
      throw new Bad('payoff.kinked strike must lie inside the scalar range');
    }
    // The payoff is normalised by the distance from strike to the end of the
    // range it pays into. A strike sitting on that end makes every outcome
    // worth nothing and the normalisation divide by zero.
    if (d.payoff.direction === 'call' && d.payoff.strike >= max) {
      throw new Bad('a call struck at the top of the range can never pay');
    }
    if (d.payoff.direction === 'put' && d.payoff.strike <= min) {
      throw new Bad('a put struck at the bottom of the range can never pay');
    }
  }

  // A deadline carries no information about anything except whether an
  // obligation was discharged in time, so it can only resolve a market whose
  // payoff is a seizure. Any other payoff needs a source of truth.
  if (d.resolution.kind === 'deadline' && py !== 'seizure') {
    throw new Bad('resolution.deadline only resolves a seizure payoff; use attestation');
  }

  // Reading another market's state yields a yes or a no, so the reading market
  // needs exactly two legs to put that answer into.
  if (d.resolution.kind === 'market' && d.positions.legs.length !== 2) {
    throw new Bad('resolution.market needs exactly two legs: the referenced condition either held or it did not');
  }

  if (py === 'linear' && d.resolution.kind !== 'attestation') {
    throw new Bad('payoff.linear needs an attested value');
  }
  if (py === 'kinked' && d.resolution.kind !== 'attestation') {
    throw new Bad('payoff.kinked needs an attested value');
  }
  if (d.mechanism.kind === 'lmsr' && pk === 'scalar') {
    throw new Bad('mechanism.lmsr supports binary and categorical positions only');
  }
  if (d.resolution.kind === 'deadline' && d.resolution.at > d.expiry) {
    throw new Bad('resolution deadline falls after expiry');
  }
}

/**
 * Reject cycles in the resolution graph.
 *
 * Recursion is the point, so this walks rather than forbids: a market may
 * resolve on a market that resolves on a market. It may not, directly or
 * transitively, resolve on itself, and it may not nest deeper than MAX_DEPTH
 * because settlement has to terminate.
 */
const MAX_DEPTH = 8;

export function checkResolutionGraph(decl, lookup, selfId = null) {
  const seen = new Set(selfId ? [selfId] : []);
  let cur = decl;
  for (let depth = 0; ; depth++) {
    if (cur.resolution.kind !== 'market') return depth;
    if (depth >= MAX_DEPTH) throw new Bad(`resolution nests deeper than ${MAX_DEPTH} markets`);
    const next = cur.resolution.market;
    if (seen.has(next)) throw new Bad('resolution graph contains a cycle');
    seen.add(next);
    const m = lookup(next);
    if (!m) throw new Bad(`resolution references unknown market ${next}`);
    cur = m.declaration;
  }
}

/**
 * Validate and normalise a declaration.
 *
 * Returns a canonical object with every field in a fixed order and defaults
 * made explicit, so that two agents who describe the same market in different
 * shorthand produce byte-identical declarations and therefore the same id.
 */
export function validate(raw, opts = {}) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new Bad('declaration must be an object');

  const unknown = Object.keys(raw).filter(
    (k) => !['collateral', 'positions', 'resolution', 'payoff', 'mechanism', 'expiry', 'label'].includes(k),
  );
  if (unknown.length) throw new Bad(`unknown field(s): ${unknown.join(', ')}`);

  const c = raw.collateral;
  if (!c || typeof c !== 'object') throw new Bad('collateral must be an object');
  if (typeof c.asset !== 'string' || !/^[A-Z][A-Z0-9]{0,15}$/.test(c.asset)) {
    throw new Bad('collateral.asset must be an uppercase ticker');
  }
  const minStake = c.min ?? 0;
  if (!finite(minStake) || minStake < 0) throw new Bad('collateral.min must be a non-negative number');

  if (!ts(raw.expiry)) throw new Bad('expiry must be a millisecond timestamp');
  if (opts.now && raw.expiry <= opts.now) throw new Bad('expiry is already in the past');

  // Free text, written by one agent and read by another. Length capped and
  // control characters stripped; it is never interpreted, only displayed.
  const label = String(raw.label ?? '').slice(0, 280).replace(/[\u0000-\u001f\u007f]/g, ' ').trim();

  const d = {
    collateral: { asset: c.asset, min: minStake },
    positions: pick(POSITIONS, 'positions', raw.positions),
    resolution: pick(RESOLUTION, 'resolution', raw.resolution),
    payoff: pick(PAYOFF, 'payoff', raw.payoff),
    mechanism: pick(MECHANISM, 'mechanism', raw.mechanism),
    expiry: raw.expiry,
    label,
  };

  coherent(d);
  return d;
}

// --- identity ---------------------------------------------------------------

/**
 * Canonical JSON: object keys in sorted order at every level, so the id is a
 * function of meaning rather than of how the declaring agent happened to
 * serialise it.
 */
export function canonical(v) {
  if (v === null || typeof v !== 'object') return JSON.stringify(v);
  if (Array.isArray(v)) return `[${v.map(canonical).join(',')}]`;
  const keys = Object.keys(v).sort();
  return `{${keys.map((k) => `${JSON.stringify(k)}:${canonical(v[k])}`).join(',')}}`;
}

/** Deterministic market id, derived from the declaration and its declarer. */
export function marketId(decl, declarer) {
  const d = new Digest();
  d.text(canonical(decl));
  d.text(declarer);
  return `mk_${d.hex()}`;
}

export const VOCABULARY = {
  positions: Object.keys(POSITIONS),
  resolution: Object.keys(RESOLUTION),
  payoff: Object.keys(PAYOFF),
  mechanism: Object.keys(MECHANISM),
  maxDepth: MAX_DEPTH,
};
