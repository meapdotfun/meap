# meap/mcp

The endpoint. An MCP server exposing a set of financial verbs that agents use
on each other.

Zero dependencies. Node 22 or newer. `node src/server.js` is the whole install.

```
npm test        42 tests
npm run demo    a loan, insurance written against it, and a default
```

## layout

```
src/grammar.js   the market vocabulary; validation, canonical ids, cycle checks
src/ledger.js    the state machine every verb moves through
src/settle.js    resolution and payoff division; pure, no state of its own
src/lmsr.js      scoring rule pricing
src/math.js      deterministic exp and log, copied from web/core
src/digest.js    two lane FNV-1a, for state digests
src/server.js    MCP over stdio, spoken directly
```

## configuration

Three environment variables. There is no config file and no account system.

| variable | default | meaning |
|---|---|---|
| `MEAP_AGENT` | required | the identity this connection acts as |
| `MEAP_STATE` | `meap.log` | action log to append to and replay from |
| `MEAP_STAKES` | `off` | `off` runs the playground, where `fund` works |

`MEAP_AGENT` is the whole of the login. Whatever value it carries is who you
are, which is what makes this machines only in practice rather than in the
marketing: there is nothing to sign up for and nobody to approve you.

## the log is the state

Every accepted action is appended to `MEAP_STATE` as one JSON line, and state
is whatever replaying that file produces. Actions carry their own timestamps,
and nothing in the ledger reads a clock or a random number, so a log replays to
the same digest on another machine months later. `audit` returns that digest.

This is why balances are integer minor units and why LMSR pricing runs on
`dexp`/`dlog` rather than `Math.exp`/`Math.log`: ECMAScript leaves those
implementation approximated, and two agents replaying the same history on
different engines would otherwise disagree about who owns what.

## known limits

- **Single process.** The reference server is one node process over a local
  file. Agents share an economy only by pointing at the same path on the same
  machine. A shared deployment needs a real backend and this is not one.
- **No chain.** Nothing here settles onchain. The design is deliberately chain
  agnostic, but no contracts exist yet.
- **Identity is a label, not a signature.** `addressOf` hashes a string.
  Anything real needs keys and signed actions.
- **`fund` creates money from nothing** while stakes are off. That is the only
  difference between the playground and a live ledger; the verb set is
  otherwise identical.
