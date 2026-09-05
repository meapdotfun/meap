/**
 * The loan story, lived out on a real chain.
 *
 * Same scenario the ledger tests and the shared-economy demo tell, executed
 * against deployed contracts: a borrower and a lender who are just addresses,
 * a loan declared as five fields, a default, and a foreclosure by the lender
 * pulling what the contract owes. Every step is a transaction anyone can look
 * up on the explorer afterwards, which is the entire difference from the
 * off-chain version: nothing here asks to be believed.
 *
 *   npx hardhat run demo.js --network robinhood
 *
 * Uses the deployer key as the borrower and derives a second throwaway as the
 * lender, funding it from the deployer. Short deadline, so the whole story
 * fits in a couple of minutes.
 */
const { ethers, network } = require("hardhat");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");

const DAY = 86_400;

async function main() {
  const file = join(__dirname, `deployment.${network.name}.json`);
  const dep = JSON.parse(readFileSync(file, "utf8"));
  console.log(`network ${network.name}, MeapMarkets ${dep.MeapMarkets}`);

  const [borrower] = await ethers.getSigners();
  const lender = ethers.Wallet.createRandom().connect(ethers.provider);
  console.log(`borrower ${borrower.address}`);
  console.log(`lender   ${lender.address} (throwaway, funded for gas)`);

  // Gas for the lender.
  await (await borrower.sendTransaction({ to: lender.address, value: ethers.parseEther("0.002") })).wait();

  const usd = await ethers.getContractAt("MeapUSD", dep.MeapUSD);
  const mkts = await ethers.getContractAt("MeapMarkets", dep.MeapMarkets);

  // Test dollars for both, straight from the faucet mint.
  await (await usd.connect(borrower).faucet(1_000_000)).wait();
  await (await usd.connect(lender).faucet(1_000_000)).wait();
  await (await usd.connect(borrower).approve(dep.MeapMarkets, ethers.MaxUint256)).wait();
  await (await usd.connect(lender).approve(dep.MeapMarkets, ethers.MaxUint256)).wait();
  console.log("both hold 1,000,000 mUSD");

  const now = (await ethers.provider.getBlock("latest")).timestamp;
  const decl = {
    token: dep.MeapUSD,
    posKind: 1, legs: 2, scalarMin: 0, scalarMax: 0,        // categorical: REPAID, DEFAULTED
    resKind: 0, deadline: now + 90, quorum: 0,               // deadline in 90 seconds
    refMarket: 0, refWhen: 0,
    payKind: 3, strike: 0, isCall: false, seizeTo: 1, discharge: 110_000,
    expiry: now + 30 * DAY,
  };

  const marketId = await mkts.connect(borrower).createMarket.staticCall(decl, [], "");
  await (await mkts.connect(borrower).createMarket(decl, [], "borrow 100000 against 150000, live on chain")).wait();
  console.log(`\nmarket ${marketId}: declared (deadline in 90s)`);

  const offerId = await mkts.connect(borrower).postOffer.staticCall(marketId, 0, 150_000, 100_000, 0);
  await (await mkts.connect(borrower).postOffer(marketId, 0, 150_000, 100_000, 0)).wait();
  console.log(`offer ${offerId}: borrower stakes 150000, asks 100000`);

  await (await mkts.connect(lender).acceptOffer(offerId)).wait();
  console.log(`lender took it: principal moved, collateral escrowed by the contract`);
  console.log(`  borrower mUSD ${await usd.balanceOf(borrower.address)}`);
  console.log(`  lender   mUSD ${await usd.balanceOf(lender.address)}`);
  console.log(`  contract mUSD ${await usd.balanceOf(dep.MeapMarkets)}`);

  console.log("\nwaiting out the deadline...");
  await new Promise((r) => setTimeout(r, 100_000));

  await (await mkts.connect(lender).settle(marketId)).wait();
  const m = await mkts.markets(marketId);
  console.log(`settled: state ${["Open", "Settled", "Defaulted", "Expired"][Number(m.state)]}, bounty paid to the settler`);

  await (await mkts.connect(lender).claim(marketId)).wait();
  console.log(`lender claimed the collateral`);
  console.log(`  borrower mUSD ${await usd.balanceOf(borrower.address)}   (kept the principal, lost the collateral)`);
  console.log(`  lender   mUSD ${await usd.balanceOf(lender.address)}   (took collateral + bounty)`);
  console.log(`  contract mUSD ${await usd.balanceOf(dep.MeapMarkets)}   (owes nobody anything)`);
}

main().catch((e) => { console.error(e); process.exit(1); });
