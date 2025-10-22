import pkg from 'hardhat';
const { ethers } = pkg as any;

async function main() {
  const [deployer] = await ethers.getSigners();
  console.log('Deployer:', deployer.address);

  const bal = await ethers.provider.getBalance(deployer.address);
  console.log('Balance:', ethers.formatEther(bal), 'BNB');

  const Factory = await ethers.getContractFactory('MeapRegistry');
  const c = await Factory.deploy();
  await c.waitForDeployment();
  const addr = await c.getAddress();
  console.log('MeapRegistry deployed:', addr);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});


