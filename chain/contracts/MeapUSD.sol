// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/**
 * Test collateral for the testnet deployment. Six decimals, so amounts line
 * up with the off-chain ledger's minor units.
 *
 * The faucet mints freely and is the loudest possible statement that this is
 * not money: anyone can have any amount, capped per call only so a typo does
 * not overflow a UI somewhere. On a mainnet deployment this contract does not
 * exist and a real stablecoin takes its place; the market engine never knew
 * the difference, which is the point of taking the token as a field of the
 * declaration.
 */
contract MeapUSD is ERC20 {
    uint256 public constant FAUCET_MAX = 1_000_000_000_000; // 1M mUSD per call

    constructor() ERC20("Meap USD (testnet)", "mUSD") {}

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function faucet(uint256 amount) external {
        require(amount <= FAUCET_MAX, "one million mUSD per call");
        _mint(msg.sender, amount);
    }
}
