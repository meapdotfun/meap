/**
 * Prove the engine executes on mainnet with a single account.
 *
 * The full two-party loan needs a second funded wallet; this needs only the
 * deployer. It mints test mUSD, approves, declares a real loan market, and
 * posts an offer on it — four real transactions on chain 4663 — then reads the
 * market back from storage to show the contract accepted and recorded it.
 *
 *   npx hardhat run scripts/prove-mainnet.js --network robinhood-mainnet
 */
const { ethers, network } = require("hardhat");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");

async function main() {
  const dep = JSON.parse(readFileSync(join(__dirname, "..", `deployment.${network.name}.json`), "utf8"));
  const [me] = await ethers.getSigners();
  const usd = await ethers.getContractAt("MeapUSD", dep.MeapUSD);
  const mkts = await ethers.getContractAt("MeapMarkets", dep.MeapMarkets);

  const link = (h) => `https://explorer.testnet.chain.robinhood.com/tx/${h}`.replace("testnet.", network.name === "robinhood-mainnet" ? "" : "testnet.");
  console.log(`account ${me.address} on chain ${dep.chainId}`);

  const t1 = await usd.faucet(1_000_000); await t1.wait();
  console.log(`faucet      ${t1.hash}`);
  const t2 = await usd.approve(dep.MeapMarkets, ethers.MaxUint256); await t2.wait();
  console.log(`approve     ${t2.hash}`);

  const now = (await ethers.provider.getBlock("latest")).timestamp;
  const decl = {
    token: dep.MeapUSD,
    posKind: 1, legs: 2, scalarMin: 0, scalarMax: 0,
    resKind: 0, deadline: now + 30 * 86400, quorum: 0, refMarket: 0, refWhen: 0,
    payKind: 3, strike: 0, isCall: false, seizeTo: 1, discharge: 110_000,
    expiry: now + 31 * 86400,
  };
  const id = await mkts.createMarket.staticCall(decl, [], "first market on mainnet");
  const t3 = await mkts.createMarket(decl, [], "first market on mainnet"); await t3.wait();
  console.log(`createMarket ${t3.hash}  -> market ${id}`);

  const offerId = await mkts.postOffer.staticCall(id, 0, 150_000, 100_000, 0);
  const t4 = await mkts.postOffer(id, 0, 150_000, 100_000, 0); await t4.wait();
  console.log(`postOffer    ${t4.hash}  -> offer ${offerId}`);

  const m = await mkts.markets(id);
  console.log(`\nread back: market ${id} declared by ${m[0]}, state ${["Open","Settled","Defaulted","Expired"][Number(m[2])]}, escrow ${m[6]}`);
  console.log(`the engine executed on Robinhood Chain mainnet.`);
}

main().catch((e) => { console.error(e.shortMessage || e.message); process.exit(1); });
