/**
 * FNV-1a carried in two 32-bit lanes.
 *
 * Lifted from web/core/hash.js, which used it to let a coordinator confirm a
 * training client's work by replaying a rollout and comparing digests. The
 * ledger wants the same property for the same reason: every action is applied
 * by a pure function, so an economy's whole history can be replayed from its
 * action log and checked against a digest. A participant cannot claim a
 * balance the log does not produce.
 *
 * Two lanes rather than one because a single 32-bit FNV collides far too
 * readily at the number of actions a busy market generates.
 */

const _f64 = new Float64Array(1);
const _u32 = new Uint32Array(_f64.buffer);

export class Digest {
  constructor() {
    this.a = 0x811c9dc5 | 0;
    this.b = 0x01000193 | 0;
  }

  u32(x) {
    this.a = Math.imul(this.a ^ (x | 0), 0x01000193);
    this.b = Math.imul(this.b + (x | 0) + 0x9e3779b9, 0x85ebca6b);
    this.b ^= this.b >>> 13;
    return this;
  }

  /** Hash a double by its bit pattern, so 0 and -0 stay distinguishable. */
  f64(x) {
    _f64[0] = x;
    return this.u32(_u32[0]).u32(_u32[1]);
  }

  /** UTF-16 code units, length-prefixed so "ab"+"c" differs from "a"+"bc". */
  text(s) {
    this.u32(s.length);
    for (let i = 0; i < s.length; i++) this.u32(s.charCodeAt(i));
    return this;
  }

  array(xs) {
    this.u32(xs.length);
    for (const x of xs) this.f64(x);
    return this;
  }

  hex() {
    const h = (v) => (v >>> 0).toString(16).padStart(8, '0');
    // Two lanes give 64 bits; repeat-mix to fill the 128-bit id space the
    // address and market-id formats expect.
    const c = Math.imul(this.a ^ this.b, 0x27d4eb2d) >>> 0;
    const d = Math.imul(this.a + this.b, 0x165667b1) >>> 0;
    return h(this.a) + h(this.b) + h(c) + h(d);
  }
}

export const digestText = (s) => new Digest().text(s).hex();
