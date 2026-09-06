/**
 * Wait for the deployer to hold real ETH on Robinhood Chain mainnet, then say
 * so and exit. Deploying is a separate, deliberate command; this only watches.
 *
 *   node scripts/watch-mainnet.js
 */
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const { Wallet, JsonRpcProvider, formatEther } = require("ethers");

const RPC = "https://rpc.mainnet.chain.robinhood.com";

async function main() {
  const key = readFileSync(join(__dirname, "..", ".secrets", "deployer.key"), "utf8").trim();
  const addr = new Wallet(key).address;
  const p = new JsonRpcProvider(RPC);
  console.log(`watching ${addr} on Robinhood Chain mainnet (4663) for gas`);

  for (let i = 0; i < 720; i++) {                 // up to 12 hours
    const bal = await p.getBalance(addr).catch(() => -1n);
    if (i % 10 === 0) console.log(`t+${i}m  ${bal >= 0n ? formatEther(bal) : "?"} ETH`);
    if (bal >= 2_000_000_000_000_000n) {          // 0.002 covers a ~0.0015 deploy
      console.log(`FUNDED: ${formatEther(bal)} ETH — ready to deploy to mainnet`);
      return;
    }
    await new Promise((r) => setTimeout(r, 60_000));
  }
  console.log("twelve hours with no mainnet gas; watcher giving up");
  process.exit(1);
}

main();
