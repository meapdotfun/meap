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
  if (isMainnet) {
    if (!collateral) throw new Error("mainnet needs MEAP_COLLATERAL set to a real ERC20 address; a faucet token is not deployed here");
    console.log(`collateral  ${collateral} (existing token, no faucet)`);
  } else if (!collateral) {
    const usd = await ethers.deployContract("MeapUSD");
    await usd.waitForDeployment();
    collateral = usd.target;
    console.log(`MeapUSD     ${collateral} (test faucet token)`);
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
    collateralIsFaucet: !isMainnet && !process.env.MEAP_COLLATERAL,
    deployedAt: new Date().toISOString(),
  };
  writeFileSync(join(__dirname, "..", `deployment.${network.name}.json`), JSON.stringify(out, null, 2) + "\n");
  console.log(`wrote deployment.${network.name}.json`);
}

main().catch((e) => { console.error(e); process.exit(1); });
