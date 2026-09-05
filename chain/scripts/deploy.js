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

  const usd = await ethers.deployContract("MeapUSD");
  await usd.waitForDeployment();
  console.log(`MeapUSD     ${usd.target}`);

  const markets = await ethers.deployContract("MeapMarkets");
  await markets.waitForDeployment();
  console.log(`MeapMarkets ${markets.target}`);

  const out = {
    network: network.name,
    chainId: Number((await ethers.provider.getNetwork()).chainId),
    rpc: network.config.url ?? "in-process",
    MeapUSD: usd.target,
    MeapMarkets: markets.target,
    deployedAt: new Date().toISOString(),
  };
  writeFileSync(join(__dirname, "..", `deployment.${network.name}.json`), JSON.stringify(out, null, 2) + "\n");
  console.log(`wrote deployment.${network.name}.json`);
}

main().catch((e) => { console.error(e); process.exit(1); });
