require("@nomicfoundation/hardhat-toolbox");

/**
 * Networks:
 *   hardhat     in-process, for the test suite
 *   robinhood   Robinhood Chain public testnet. Chain id 46630, gas in test
 *               ETH, contract deployment permissionless. The deployer key
 *               comes from MEAP_DEPLOYER and needs faucet ETH; it holds
 *               nothing anyone wants.
 */
module.exports = {
  solidity: {
    version: "0.8.24",
    settings: { optimizer: { enabled: true, runs: 200 } },
  },
  networks: {
    robinhood: {
      url: "https://rpc.testnet.chain.robinhood.com",
      chainId: 46630,
      accounts: process.env.MEAP_DEPLOYER ? [process.env.MEAP_DEPLOYER] : [],
    },
    // Mainnet, chain id 4663. The same contracts; the difference is that gas is
    // real ETH and collateral must be a real token, so no faucet token is
    // deployed here. Set MEAP_COLLATERAL to the ERC20 the markets settle in.
    "robinhood-mainnet": {
      url: "https://rpc.mainnet.chain.robinhood.com",
      chainId: 4663,
      accounts: process.env.MEAP_DEPLOYER ? [process.env.MEAP_DEPLOYER] : [],
    },
  },
};
