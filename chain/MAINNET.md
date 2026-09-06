# Mainnet readiness

What stands between the testnet deployment and real value on Robinhood Chain
mainnet (chain id 4663), stated honestly. The engineering is done; one gate is
not, and it is not an engineering gate.

## Done

- **The contract holds and moves the money, not an operator.** Collateral sits
  in `MeapMarkets`, and payouts follow the declared payoff in bytecode. There
  is no owner, no admin key, and nothing to upgrade, so there is no operator to
  trust and none to compromise.
- **Asset agnostic by construction.** Collateral is a field of every market,
  never baked into the contract. The testnet uses a free faucet token; mainnet
  uses whatever real ERC20 is passed as `MEAP_COLLATERAL`. The engine cannot
  tell the difference, which is the whole point.
- **No faucet on mainnet.** `scripts/deploy.js` refuses to deploy the faucet
  token on chain 4663 and requires a real collateral address, so free money
  cannot masquerade as real.
- **Static analysis clean.** Slither reports no reentrancy, no arbitrary send,
  no unchecked transfer. The only findings are `block.timestamp` comparisons
  (correct for deadline markets, and documented in the source) and cyclomatic
  complexity (inherent to a grammar validator).
- **Reentrancy proven, not assumed.** A malicious collateral token that calls
  back into `claim()` during its own payout is in the test suite, and it takes
  nothing: the claimer is marked paid before any transfer, and `nonReentrant`
  guards every path that moves tokens.
- **Full-precision money math.** Division uses OpenZeppelin `Math.mulDiv`, so a
  market with extreme stakes divides correctly rather than reverting on an
  intermediate overflow.
- **Conservation holds.** The contract's token balance always equals the sum of
  open escrows and unclaimed payouts; 21 tests end with the contract owing
  nothing and no token created or destroyed.

## The gate

**An external audit.** 21 tests, a matching off-chain engine, and a clean
static-analysis pass are strong evidence and not a substitute for a firm whose
job is to break this for a living. A bug that holds real funds is irreversible.
This is money and weeks, not a command, and it is a decision for the project to
make, not something code can supply.

Until then, real value should not sit in these contracts. The testnet
deployment is the honest place for them to live and be used.

## When the gate is cleared

Deploying to mainnet is then one command, against a real collateral token:

```
MEAP_DEPLOYER=<funded key> \
MEAP_COLLATERAL=<real ERC20 on Robinhood Chain> \
npx hardhat run scripts/deploy.js --network robinhood-mainnet
```

The deployer needs real ETH for gas. The contracts have no owner, so once
deployed the deployer key holds no special power over them.
