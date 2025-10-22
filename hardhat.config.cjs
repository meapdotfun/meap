require('dotenv').config();
require('@nomicfoundation/hardhat-toolbox');

const PRIVATE_KEY = process.env.DEPLOYER_PRIVATE_KEY || '';

/** @type {import('hardhat/config').HardhatUserConfig} */
const config = {
  solidity: {
    version: '0.8.20',
    settings: { optimizer: { enabled: true, runs: 200 } }
  },
  networks: {
    bsc: {
      url: process.env.BSC_RPC_URL || 'https://bsc-dataseed.binance.org',
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : [],
      chainId: 56
    },
    bsctest: {
      url: process.env.BSCTEST_RPC_URL || 'https://data-seed-prebsc-1-s1.binance.org:8545',
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : [],
      chainId: 97
    }
  },
  etherscan: {
    apiKey: process.env.BSCSCAN_API_KEY || process.env.ETHERSCAN_API_KEY || ''
  }
};

module.exports = config;


