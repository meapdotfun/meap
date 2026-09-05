/**
 * Wait for the deployer to be funded, on either chain, then say so and exit.
 *
 * The deploy itself is deliberately not automated from here: the watcher's job
 * is to notice, not to act. It exits the moment gas appears so whoever is
 * driving can run the bridge and the deploy with fresh eyes.
 *
 *   node scripts/watch-funding.js
 */
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const { Wallet, JsonRpcProvider, formatEther } = require("ethers");

const SEPOLIA = "https://ethereum-sepolia-rpc.publicnode.com";
const ROBINHOOD = "https://rpc.testnet.chain.robinhood.com";

async function main() {
  const key = readFileSync(join(__dirname, "..", ".secrets", "deployer.key"), "utf8").trim();
  const addr = new Wallet(key).address;
  const l1 = new JsonRpcProvider(SEPOLIA);
  const l2 = new JsonRpcProvider(ROBINHOOD);
  console.log(`watching ${addr} on Sepolia and Robinhood testnet`);

  for (let i = 0; i < 360; i++) {                       // up to 6 hours
    const [s, r] = await Promise.all([
      l1.getBalance(addr).catch(() => -1n),
      l2.getBalance(addr).catch(() => -1n),
    ]);
    const line = `t+${i}m sepolia ${s >= 0n ? formatEther(s) : "?"} | robinhood ${r >= 0n ? formatEther(r) : "?"}`;
    if (i % 10 === 0) console.log(line);

    if (r > 0n) {
      console.log(`FUNDED on Robinhood testnet: ${formatEther(r)} ETH — ready to deploy`);
      return;
    }
    if (s >= 3_000_000_000_000_000n) {                  // 0.003 covers deposit + gas
      console.log(`FUNDED on Sepolia: ${formatEther(s)} ETH — ready to bridge (scripts/bridge.js)`);
      return;
    }
    await new Promise((res) => setTimeout(res, 60_000));
  }
  console.log("six hours with no funds; watcher giving up");
  process.exit(1);
}

main();
