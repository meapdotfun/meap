/**
 * Move Sepolia ETH onto Robinhood Chain testnet.
 *
 * Robinhood Chain is an Arbitrum Orbit chain whose testnet parent is Ethereum
 * Sepolia, so funding it is a call to the chain's Delayed Inbox on Sepolia:
 * depositEth() credits the same address on the child chain once the deposit is
 * picked up, usually inside fifteen minutes. Addresses are from
 * docs.robinhood.com/chain/protocol-contracts.
 *
 *   node scripts/bridge.js 0.03          # deposit 0.03 Sepolia ETH
 *
 * Reads the key from ../.secrets/deployer.key, then watches both balances
 * until the funds land.
 */
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const { Wallet, JsonRpcProvider, Contract, parseEther, formatEther } = require("ethers");

const INBOX = "0xF2939afA86F6f933A3CE17fCAB007907B6b0B7a4";
const SEPOLIA = "https://ethereum-sepolia-rpc.publicnode.com";
const ROBINHOOD = "https://rpc.testnet.chain.robinhood.com";

async function main() {
  const amount = parseEther(process.argv[2] || "0.03");
  const key = readFileSync(join(__dirname, "..", ".secrets", "deployer.key"), "utf8").trim();

  const l1 = new Wallet(key, new JsonRpcProvider(SEPOLIA));
  const l2 = new JsonRpcProvider(ROBINHOOD);

  const [l1bal, l2bal] = await Promise.all([
    l1.provider.getBalance(l1.address), l2.getBalance(l1.address),
  ]);
  console.log(`address  ${l1.address}`);
  console.log(`sepolia  ${formatEther(l1bal)} ETH`);
  console.log(`robinhood ${formatEther(l2bal)} ETH`);
  if (l1bal < amount) throw new Error(`not enough on Sepolia to deposit ${formatEther(amount)}`);

  const inbox = new Contract(INBOX, ["function depositEth() payable returns (uint256)"], l1);
  const tx = await inbox.depositEth({ value: amount });
  console.log(`deposit  ${tx.hash}`);
  await tx.wait();
  console.log("mined on Sepolia; waiting for the child chain to credit it...");

  for (let i = 0; i < 120; i++) {
    await new Promise((r) => setTimeout(r, 30_000));
    const now = await l2.getBalance(l1.address);
    process.stdout.write(`  t+${(i + 1) / 2}min robinhood ${formatEther(now)} ETH\r\n`);
    if (now > l2bal) {
      console.log("landed.");
      return;
    }
  }
  throw new Error("deposit did not arrive within an hour; check the inbox tx on Sepolia");
}

main().catch((e) => { console.error(e.message); process.exit(1); });
