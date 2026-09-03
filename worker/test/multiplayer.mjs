/**
 * Two agents, one economy, over HTTP.
 *
 * Point it at a running endpoint and it registers two independent agents, has
 * one declare a loan and post an offer, and checks the other can see and take
 * it. That last step is the whole difference from the local build, where each
 * caller gets a private ledger nobody else can reach.
 *
 *   npx wrangler dev --port 8788 --local
 *   node worker/test/multiplayer.mjs http://127.0.0.1:8788
 *
 * Not part of `npm test`, which stays dependency free and runs the ledger,
 * the grammar and the stdio transport in process. This one needs something
 * listening.
 */
const BASE = process.argv[2] || 'http://127.0.0.1:8788';

async function register() {
  const r = await fetch(`${BASE}/register`, { method: 'POST' });
  return r.json();
}

function agent(token) {
  let id = 0;
  return async (method, params) => {
    const r = await fetch(`${BASE}/mcp`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
      body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }),
    });
    if (r.status === 202) return null;
    return r.json();
  };
}

const call = async (rpc, name, args) => {
  const r = await rpc('tools/call', { name, arguments: args });
  const text = r.result.content[0].text;
  let json = null;
  try { json = JSON.parse(text); } catch {}
  return { isError: !!r.result.isError, text, json };
};

const log = (s) => console.log(s);

// --- two agents, registered independently -----------------------------------

const A = await register();          // borrower
const B = await register();          // lender
log(`borrower ${A.address}`);
log(`lender   ${B.address}`);
if (A.address === B.address) throw new Error('two registrations produced one address');

const a = agent(A.token);
const b = agent(B.token);

// --- handshake --------------------------------------------------------------

const init = await a('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'test', version: '0' } });
log(`\ninitialize -> ${init.result.serverInfo.name} ${init.result.serverInfo.version}, protocol ${init.result.protocolVersion}`);
const list = await a('tools/list');
log(`tools      -> ${list.result.tools.length}`);

const meA = await call(a, 'whoami');
const meB = await call(b, 'whoami');
log(`\nborrower opens with ${meA.json.balances.USD} USD, stakes ${meA.json.stakes}`);
log(`lender   opens with ${meB.json.balances.USD} USD`);

// fund must be refused on the shared economy
const minted = await call(a, 'fund', { asset: 'USD', amount: 10 ** 9 });
log(`fund     -> ${minted.text.split('\n')[0]}`);
if (!minted.isError) throw new Error('the shared economy let someone mint');

// --- the borrower opens a loan ----------------------------------------------

const now = Date.now();
const made = await call(a, 'create_market', {
  declaration: {
    collateral: { asset: 'USD' },
    positions: { kind: 'categorical', outcomes: ['REPAID', 'DEFAULTED'] },
    resolution: { kind: 'deadline', at: now + 2000 },
    payoff: { kind: 'seizure', to: 'DEFAULTED', discharge: 110_000 },
    mechanism: { kind: 'bilateral' },
    expiry: now + 120_000,
    label: 'borrow 100000 against 150000',
  },
});
if (made.isError) throw new Error(made.text);
const market = made.json.market;
log(`\nborrower declared ${market}`);

const offered = await call(a, 'post_offer', { market, leg: 'REPAID', stake: 150_000, ask: 100_000, counter_stake: 0 });
if (offered.isError) throw new Error(offered.text);
log(`borrower posted  ${offered.json.offer}, escrowed ${offered.json.escrowed}`);

// --- the LENDER sees it. this is the whole point ----------------------------

const seen = await call(b, 'list_offers', {});
log(`\nlender sees ${seen.json.length} offer(s) from another agent`);
if (seen.json.length !== 1) throw new Error('the lender cannot see the borrower: not one economy');
if (seen.json[0].from !== A.address) throw new Error('offer is not attributed to the borrower');

const taken = await call(b, 'accept_offer', { offer: offered.json.offer });
if (taken.isError) throw new Error(taken.text);
log(`lender took it, holding ${taken.json.youHold.shares} of ${taken.json.youHold.leg}`);

// --- the deadline passes, anyone may foreclose ------------------------------

const early = await call(b, 'foreclose', { market });
log(`\nforeclose early -> ${early.text.split('\n')[0].slice(0, 60)}`);

await new Promise((r) => setTimeout(r, 2200));
const seized = await call(b, 'foreclose', { market });
if (seized.isError) throw new Error(seized.text);
log(`foreclose late  -> ${seized.json.state}, bounty ${seized.json.bounty}`);

// --- the books ---------------------------------------------------------------

const endA = await call(a, 'whoami');
const endB = await call(b, 'whoami');
log(`\nborrower ${endA.json.balances.USD}  (kept the principal, lost the collateral)`);
log(`lender   ${endB.json.balances.USD}  (took the collateral)`);

const state = await (await fetch(`${BASE}/state`)).json();
log(`\npublic /state: ${state.agents.length} agents, ${state.markets.length} market(s), ${state.actions} actions`);
log(`total on the ledger: ${state.totals.USD}`);
log(`digest: ${state.digest}`);

// The only way value enters is the opening grant, so the total is a count of
// arrivals times the grant. Checked against however many agents the economy
// holds rather than the two this run added, since it may already have a past.
const expected = state.agents.length * state.opening.amount;
log(`expected: ${state.agents.length} arrivals x ${state.opening.amount} = ${expected}`);
if (state.totals.USD !== expected) throw new Error(`value leaked: ${state.totals.USD} vs ${expected}`);
log('\nOK');
