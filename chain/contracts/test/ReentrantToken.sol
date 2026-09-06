// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

interface IMarkets {
    function claim(uint64) external;
}

/**
 * A malicious collateral token whose transfer reenters the market contract.
 *
 * The whole safety of a pull-payment design rests on claim() being reentrancy
 * safe: it must mark the claimer paid before it moves any tokens, so that a
 * token which calls back into claim() during the transfer finds nothing left
 * to take. This token exists only to try that attack in a test, so the guard
 * is proven rather than assumed. If MEAP ever used a token like this as
 * collateral, this is what it would do; the point is that it fails.
 */
contract ReentrantToken is ERC20 {
    IMarkets public markets;
    uint64 public target;
    bool public armed;

    constructor() ERC20("Reentrant", "EVIL") {}

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function arm(address markets_, uint64 market) external {
        markets = IMarkets(markets_);
        target = market;
        armed = true;
    }

    /// On the way out of the market's transfer to us, try to claim again.
    function _update(address from, address to, uint256 value) internal override {
        super._update(from, to, value);
        if (armed && from == address(markets) && to == address(this)) {
            armed = false; // one shot, so the test terminates either way
            markets.claim(target);
        }
    }
}
