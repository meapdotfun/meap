/**
 * Declare a real, USDG-collateralised loan market on Robinhood Chain mainnet.
 *
 * USDG (Global Dollar, Paxos, 1:1 USD) is the chain's real stablecoin. Passing
 * its address as the collateral token is all it takes for a market to settle in
 * real dollars: MeapMarkets is asset agnostic, so nothing about the contract
 * changes. This is a bilateral declaration, so it moves no tokens and holds
 * nothing; it stands open until someone with USDG posts and takes an offer on
 * it. The point is to show that a real-dollar market is creatable on the live
 * contract, today, with the same code the test-token demo ran on.
 *
 *   npx hardhat run scripts/usdg-market.js --network robinhood-mainnet
 */
const { ethers, network } = require("hardhat");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");

const USDG = "0x5fc5360D0400a0Fd4f2af552ADD042D716F1d168"; // Global Dollar, 6 decimals

async function main() {
  if (network.name !== "robinhood-mainnet") throw new Error("this market is for mainnet");
  const dep = JSON.parse(readFileSync(join(__dirname, "..", "deployment.robinhood-mainnet.json"), "utf8"));
  const mkts = await ethers.getContractAt("MeapMarkets", dep.MeapMarkets);

  const usd = (n) => BigInt(n) * 1_000_000n; // USDG has 6 decimals
  const now = (await ethers.provider.getBlock("latest")).timestamp;

  // Borrow 1,000 USDG against 1,500 USDG collateral, 30 days, repay 1,100.
  const decl = {
    token: USDG,
    posKind: 1, legs: 2, scalarMin: 0, scalarMax: 0,        // REPAID, DEFAULTED
    resKind: 0, deadline: now + 30 * 86400, quorum: 0, refMarket: 0, refWhen: 0,
    payKind: 3, strike: 0, isCall: false, seizeTo: 1, discharge: usd(1100),
    expiry: now + 31 * 86400,
  };

  const id = await mkts.createMarket.staticCall(decl, [], "borrow 1000 USDG against 1500, 30 days — real dollars");
  const tx = await mkts.createMarket(decl, [], "borrow 1000 USDG against 1500, 30 days — real dollars");
  await tx.wait();

  const m = await mkts.markets(id);
  console.log(`market ${id} declared: collateral token ${m[1][0]}`);
  console.log(`  = USDG (${USDG}): ${m[1][0].toLowerCase() === USDG.toLowerCase()}`);
  console.log(`  state ${["Open","Settled","Defaulted","Expired"][Number(m[2])]}, holds nothing until an offer is funded`);
  console.log(`  tx    ${tx.hash}`);
  console.log(`\na real-dollar loan market is live on Robinhood Chain mainnet.`);
}

main().catch((e) => { console.error(e.shortMessage || e.message); process.exit(1); });
