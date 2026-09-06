/**
 * Deploy the token and the market engine, and write the addresses where the
 * demo, the bridge and the site can find them.
 *
 *   npx hardhat run scripts/deploy.js --network robinhood
 *
 * MEAP_DEPLOYER must hold a little test ETH for gas. The key deploys and is
 * then ordinary: the contracts have no owner, no admin, and nothing to
 * upgrade, so there is nothing for a deployer to be trusted with afterwards.
 */
const { ethers, network } = require("hardhat");
const { writeFileSync } = require("node:fs");
const { join } = require("node:path");

async function main() {
  const [deployer] = await ethers.getSigners();
  if (!deployer) throw new Error("set MEAP_DEPLOYER to a funded private key");
  const balance = await ethers.provider.getBalance(deployer.address);
  console.log(`deployer ${deployer.address} on ${network.name} (${balance} wei)`);

  // The market engine is asset agnostic: collateral is a field of every market,
  // never baked into the contract. So the only real-vs-test difference is which
  // token backs the markets. On mainnet a faucet token would be free money
  // pretending to be real, so it is refused; supply a genuine ERC20 instead.
  const isMainnet = Number((await ethers.provider.getNetwork()).chainId) === 4663;
  let collateral = process.env.MEAP_COLLATERAL;
  let collateralIsFaucet = false;

  // MeapMarkets is asset agnostic: collateral is a field of every market, so
  // once the engine is on a chain, any market can settle in any real token
  // that exists there. That makes the engine itself the deliverable. A default
  // collateral token is a convenience for demos and nothing more.
  //
  // On mainnet the default is a real ERC20 via MEAP_COLLATERAL. Deploying the
  // free faucet token there is allowed only with MEAP_ALLOW_TEST_TOKEN=1, and
  // it is recorded as a faucet so nothing can later mistake it for real value.
  if (collateral) {
    console.log(`collateral  ${collateral} (existing token)`);
  } else if (isMainnet && process.env.MEAP_ALLOW_TEST_TOKEN !== "1") {
    throw new Error(
      "mainnet: set MEAP_COLLATERAL to a real ERC20, or MEAP_ALLOW_TEST_TOKEN=1 to " +
      "deploy the free faucet token (which is worth nothing and will be labelled so)");
  } else {
    const usd = await ethers.deployContract("MeapUSD");
    await usd.waitForDeployment();
    collateral = usd.target;
    collateralIsFaucet = true;
    console.log(`MeapUSD     ${collateral} (FREE FAUCET TOKEN, worth nothing${isMainnet ? " — on mainnet by explicit opt-in" : ""})`);
  }

  const markets = await ethers.deployContract("MeapMarkets");
  await markets.waitForDeployment();
  console.log(`MeapMarkets ${markets.target}`);

  const out = {
    network: network.name,
    chainId: Number((await ethers.provider.getNetwork()).chainId),
    rpc: network.config.url ?? "in-process",
    MeapUSD: collateral,
    MeapMarkets: markets.target,
    collateralIsFaucet,
    deployedAt: new Date().toISOString(),
  };
  writeFileSync(join(__dirname, "..", `deployment.${network.name}.json`), JSON.stringify(out, null, 2) + "\n");
  console.log(`wrote deployment.${network.name}.json`);
}

main().catch((e) => { console.error(e); process.exit(1); });
