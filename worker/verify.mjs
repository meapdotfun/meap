/**
 * Check the economy against its own log.
 *
 * Fetches every action, replays it locally with the same ledger the server
 * runs, and compares the digest to the one the server reports. If they match,
 * the balances it publishes are the balances its history produces, and it has
 * proved that rather than asked to be believed.
 *
 * This is also the backup: the file it writes is a complete copy, because
 * state here is nothing but the log replayed.
 *
 *   node worker/verify.mjs https://mcp.meap.fun [backup.json]
 */

import { writeFileSync } from 'node:fs';
import { Ledger } from '../mcp/src/ledger.js';

const BASE = process.argv[2] || 'https://mcp.meap.fun';
const OUT = process.argv[3];

const [log, state] = await Promise.all([
  fetch(`${BASE}/log`).then((r) => r.json()),
  fetch(`${BASE}/state`).then((r) => r.json()),
]);

console.log(`${log.actions.length} actions, ${state.agents.length} agents, ${state.markets.length} markets`);
console.log(`supply  ${log.genesis.treasury.amount} ${log.genesis.treasury.asset} at genesis`);

const replayed = Ledger.replay(log.actions, {
  playground: false,
  opening: log.genesis.opening,
  treasury: log.genesis.treasury,
});

const mine = replayed.digest();
console.log(`\nserver digest   ${state.digest}`);
console.log(`replay digest   ${mine}`);
console.log(mine === state.digest ? 'MATCH: the published state is what the log produces'
  : 'DIVERGED: the server is reporting something its own history does not produce');

// The supply is fixed at genesis, so the total can never move off it.
const total = replayed.audit().get(log.genesis.treasury.asset) ?? 0;
console.log(`\nsupply now      ${total}`);
console.log(total === log.genesis.treasury.amount
  ? 'CONSERVED: nothing was created or destroyed'
  : `LEAKED: ${total - log.genesis.treasury.amount}`);

if (OUT) {
  writeFileSync(OUT, JSON.stringify(log, null, 2));
  console.log(`\nwrote ${OUT}`);
}

if (mine !== state.digest || total !== log.genesis.treasury.amount) process.exit(1);
