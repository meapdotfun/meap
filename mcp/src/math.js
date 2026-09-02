/**
 * Copied verbatim from web/core/math.js so that mcp/ runs standalone; an agent
 * that clones only this directory must still price identically to every other
 * agent. Do not edit here. The reason it matters for a ledger is the same
 * reason it mattered for the simulation: LMSR cost is b*ln(sum(exp(q/b))), and
 * Math.exp/Math.log disagree in the last ulp across engines, so two agents
 * replaying the same action log would compute different balances.
 */

/**
 * Deterministic elementary functions.
 *
 * ECMAScript specifies +, -, *, /, and Math.sqrt as IEEE-754 double operations
 * with correctly-rounded results, so they are bit-identical on every conforming
 * engine.  It does NOT specify Math.sin, Math.cos, Math.exp, Math.log, Math.pow
 * or Math.tanh: "the choice of algorithms is implementation-approximated"
 * (ECMA-262 21.3.2).  V8, SpiderMonkey and JavaScriptCore genuinely disagree in
 * the last ulp on some inputs.
 *
 * A single ulp of disagreement inside an integrator diverges over a few hundred
 * steps, which would break (a) reproducible rollouts from a seed and (b) the
 * duplicate-assignment verification the training pool relies on.  So every
 * transcendental used inside the simulation is implemented here on top of the
 * exactly-specified operations only.
 *
 * Kernels follow fdlibm (Sun Microsystems, 1993) for sin/cos/log and Cephes
 * (Moshier, 1989) for tanh; exp uses Cody-Waite argument reduction with a
 * degree-13 Taylor kernel, whose truncation error on the reduced range
 * |r| <= ln(2)/2 is bounded by r^14/14! < 5e-18.
 *
 * Accuracy is asserted against Math.* in tools/test-core.js (max relative
 * error < 2e-15 over 2e5 samples per function).
 */

// Exact powers of two.  Every 2^k in [-1074, 1023] is representable, and the
// repeated halving/doubling used to build the table is itself exact.
const POW2 = new Float64Array(2098); // index k + 1074
{
  let p = 1;
  for (let k = 0; k <= 1023; k++) { POW2[k + 1074] = p; p *= 2; }
  p = 1;
  for (let k = -1; k >= -1074; k--) { p *= 0.5; POW2[k + 1074] = p; }
}

/** x * 2^k, exact for the ranges used here. */
export function ldexp(x, k) {
  if (k > 1023) { x *= POW2[1023 + 1074]; k -= 1023; if (k > 1023) return x * Infinity; }
  else if (k < -1074) return x * 0;
  return x * POW2[k + 1074];
}

// --- bit access -------------------------------------------------------------
const _f64 = new Float64Array(1);
const _u32 = new Uint32Array(_f64.buffer); // [0] = low word, [1] = high word (LE)

// --- exp --------------------------------------------------------------------
const INV_LN2 = 1.4426950408889634;
const LN2_HI = 6.93147180369123816490e-01; // ln2 rounded to 32 significant bits
const LN2_LO = 1.90821492927058770002e-10; // ln2 - LN2_HI

const EXP_C = new Float64Array(14);
{
  let f = 1;
  EXP_C[0] = 1;
  for (let n = 1; n < 14; n++) { f *= n; EXP_C[n] = 1 / f; }
}

export function dexp(x) {
  if (x !== x) return NaN;
  if (x > 709.782712893384) return Infinity;
  if (x < -745.1332191019411) return 0;
  const k = Math.round(x * INV_LN2);
  // Cody-Waite: subtract k*ln2 in two pieces so the residual keeps full precision.
  const r = (x - k * LN2_HI) - k * LN2_LO;
  let y = EXP_C[13];
  for (let i = 12; i >= 0; i--) y = y * r + EXP_C[i];
  return ldexp(y, k);
}

// --- log --------------------------------------------------------------------
const LG1 = 6.666666666666735130e-01, LG2 = 3.999999999940941908e-01,
      LG3 = 2.857142874366239149e-01, LG4 = 2.222219843214978396e-01,
      LG5 = 1.818357216161805012e-01, LG6 = 1.531383769920937332e-01,
      LG7 = 1.479819860511658591e-01;
const SQRT2 = 1.4142135623730951;

export function dlog(x) {
  if (x !== x) return NaN;
  if (x < 0) return NaN;
  if (x === 0) return -Infinity;
  if (x === Infinity) return Infinity;
  let e = 0;
  if (x < 2.2250738585072014e-308) { x *= POW2[54 + 1074]; e -= 54; } // lift subnormals
  _f64[0] = x;
  e += ((_u32[1] >>> 20) & 0x7ff) - 1023;
  _u32[1] = (_u32[1] & 0x000fffff) | 0x3ff00000; // mantissa into [1, 2)
  let m = _f64[0];
  if (m > SQRT2) { m *= 0.5; e += 1; }           // into [sqrt2/2, sqrt2)
  const f = m - 1;
  const s = f / (2 + f);
  const z = s * s;
  const w = z * z;
  const t1 = w * (LG2 + w * (LG4 + w * LG6));
  const t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
  const R = t1 + t2;
  return e * LN2_HI - ((s * (f - R) - e * LN2_LO) - f);
}

// --- sin / cos --------------------------------------------------------------
const S1 = -1.66666666666666324348e-01, S2 = 8.33333333332248946124e-03,
      S3 = -1.98412698298579493134e-04, S4 = 2.75573137070700676789e-06,
      S5 = -2.50507602534068634195e-08, S6 = 1.58969099521155010221e-10;
const C1 = 4.16666666666666019037e-02, C2 = -1.38888888888741095749e-03,
      C3 = 2.48015872894767294178e-05, C4 = -2.75573143513906633035e-07,
      C5 = 2.08757232129817482790e-09, C6 = -1.13596475577881948265e-11;
const PIO2_1 = 1.57079632673412561417e+00;  // pi/2, high 33 bits
const PIO2_1T = 6.07710050650619224932e-11; // pi/2 - PIO2_1
const TWO_OVER_PI = 6.36619772367581382433e-01;

function sinKernel(x) {
  const z = x * x;
  return x + x * z * (S1 + z * (S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)))));
}
function cosKernel(x) {
  const z = x * x;
  return 1 - 0.5 * z + z * z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
}

// Two-term Cody-Waite reduction.  Exact enough while |x| stays modest: the
// dropped third term contributes < 1e-19 for |x| < 1e5, and every angle that
// reaches these functions is wrapped to [-pi, pi] first.
function reduce(x) {
  const n = Math.round(x * TWO_OVER_PI);
  return { n: n | 0, r: (x - n * PIO2_1) - n * PIO2_1T };
}

export function dsin(x) {
  if (!isFinite(x)) return NaN;
  const { n, r } = reduce(x);
  switch (n & 3) {
    case 0: return sinKernel(r);
    case 1: return cosKernel(r);
    case 2: return -sinKernel(r);
    default: return -cosKernel(r);
  }
}

export function dcos(x) {
  if (!isFinite(x)) return NaN;
  const { n, r } = reduce(x);
  switch (n & 3) {
    case 0: return cosKernel(r);
    case 1: return -sinKernel(r);
    case 2: return -cosKernel(r);
    default: return sinKernel(r);
  }
}

/** sin and cos of the same angle, one reduction. */
export function dsincos(x, out) {
  const { n, r } = reduce(x);
  const s = sinKernel(r), c = cosKernel(r);
  switch (n & 3) {
    case 0: out[0] = s; out[1] = c; break;
    case 1: out[0] = c; out[1] = -s; break;
    case 2: out[0] = -s; out[1] = -c; break;
    default: out[0] = -c; out[1] = s; break;
  }
  return out;
}

// --- tanh (Cephes) ----------------------------------------------------------
const TP0 = -9.64399179425052238628e-1, TP1 = -9.92877231001918586564e1,
      TP2 = -1.61468768441708447952e3;
const TQ0 = 1.12811678491632931402e2, TQ1 = 2.23548839060100448583e3,
      TQ2 = 4.84406305325125486048e3;

export function dtanh(x) {
  const a = x < 0 ? -x : x;
  if (a >= 20) return x < 0 ? -1 : 1;          // 2/(e^40+1) < 2^-56, rounds to 1
  if (a > 0.625) {
    const s = dexp(a + a);
    const z = 1 - 2 / (s + 1);
    return x < 0 ? -z : z;
  }
  if (x === 0) return x;
  const s = x * x;
  const num = (TP0 * s + TP1) * s + TP2;
  const den = ((s + TQ0) * s + TQ1) * s + TQ2;
  return x + x * s * (num / den);
}

// --- misc -------------------------------------------------------------------
export const PI = 3.141592653589793;
export const TAU = 6.283185307179586;

/** Wrap to [-pi, pi).  Keeps the argument of dsin/dcos bounded. */
export function wrapPi(a) {
  return a - TAU * Math.round(a / TAU);
}

export function clamp(x, lo, hi) { return x < lo ? lo : (x > hi ? hi : x); }
