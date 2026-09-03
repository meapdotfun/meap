#!/usr/bin/env node
/**
 * The local endpoint: MCP over stdio.
 *
 * JSON-RPC 2.0, newline delimited, requests on stdin and responses on stdout.
 * Nothing else may ever be written to stdout, because a stray console.log
 * corrupts the framing and the session dies; diagnostics go to stderr.
 *
 * This file is only a transport. The verbs live in tools.js and the rules in
 * ledger.js, both of which the hosted worker uses unchanged, so a local
 * playground and the shared economy cannot drift apart.
 *
 * Running this gives you a private economy over a local file. Nobody else can
 * see it and nobody else can trade in it; for that, point an agent at the
 * shared endpoint instead. This is the version to experiment against, where
 * `fund` works and mistakes cost nothing.
 *
 * Identity comes from the configuration, not from a signup. Whatever
 * MEAP_AGENT carries is who you are, which is the whole of the account system.
 *
 *   MEAP_AGENT    identity for this connection. Required.
 *   MEAP_STATE    action log to append to and replay from. Default ./meap.log
 *   MEAP_STAKES   'off' (default) allows fund. 'on' removes it.
 */

import { createInterface } from 'node:readline';
import { appendFileSync, readFileSync, existsSync } from 'node:fs';

import { Ledger, addressOf } from './ledger.js';
import { makeTools, dispatch, VERSION } from './tools.js';

const AGENT = process.env.MEAP_AGENT;
const STATE = process.env.MEAP_STATE || 'meap.log';
const PLAYGROUND = (process.env.MEAP_STAKES || 'off') !== 'on';

if (!AGENT) {
  process.stderr.write('MEAP_AGENT is not set. That value is the identity this connection acts as.\n');
  process.exit(2);
}
const ME = addressOf(AGENT);

// --- durability -------------------------------------------------------------

/**
 * The log is the ledger. State is whatever replaying it produces, which is why
 * every action carries its own timestamp: the file has to be replayable on a
 * different machine, months later, to the same digest.
 */
const ledger = new Ledger({ playground: PLAYGROUND });
let restored = 0;
if (existsSync(STATE)) {
  for (const line of readFileSync(STATE, 'utf8').split('\n')) {
    if (!line.trim()) continue;
    try { ledger.apply(JSON.parse(line)); restored++; } catch (e) {
      process.stderr.write(`skipping unreplayable action at line ${restored + 1}: ${e.message}\n`);
    }
  }
}

function commit(action) {
  const result = ledger.apply(action);          // throws before anything is written
  appendFileSync(STATE, JSON.stringify(action) + '\n');
  return result;
}

/** Join on first contact, so an address exists the moment it connects. */
if (!ledger.agents.has(ME)) commit({ type: 'join', by: ME, at: Date.now() });

const tools = makeTools({
  ledger, me: ME, commit, now: () => Date.now(), playground: PLAYGROUND,
});

// --- transport --------------------------------------------------------------

const rl = createInterface({ input: process.stdin });
rl.on('line', (line) => {
  if (!line.trim()) return;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } }) + '\n');
    return;
  }
  try {
    const reply = dispatch(tools, msg, { name: 'meap' });
    if (reply) process.stdout.write(JSON.stringify(reply) + '\n');
  } catch (e) {
    process.stderr.write(`${e.stack}\n`);
    if (msg.id !== undefined) {
      process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: e.message } }) + '\n');
    }
  }
});

process.stderr.write(
  `meap ${VERSION} up as ${ME} (${AGENT}), stakes ${PLAYGROUND ? 'off' : 'on'}, ` +
  `${restored} actions replayed from ${STATE}\n`,
);
