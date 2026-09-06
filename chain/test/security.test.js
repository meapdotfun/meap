/**
 * The attacks a mainnet deployment has to survive, run for real.
 *
 * Slither found no reentrancy, no arbitrary send and no unchecked transfer, and
 * that is the important result. These tests are the belt to that suspenders:
 * they mount the attacks a static analyser reasons about abstractly and show
 * them failing against the compiled contract, so the guarantee is demonstrated
 * rather than argued.
 */
const { expect } = require("chai");
const { ethers } = require("hardhat");
const { time } = require("@nomicfoundation/hardhat-network-helpers");

const DAY = 86_400;
const Pos = { Categorical: 1 };
const Res = { Deadline: 0 };
const Pay = { Seizure: 3 };

describe("security", () => {
  let mkts, borrower, lender, watcher;

  beforeEach(async () => {
    [borrower, lender, watcher] = await ethers.getSigners();
    mkts = await ethers.deployContract("MeapMarkets");
  });

  it("a token that reenters claim() cannot be paid twice", async () => {
    const evil = await ethers.deployContract("ReentrantToken");
    for (const who of [borrower, lender]) {
      await evil.mint(who.address, 1_000_000);
      await evil.connect(who).approve(mkts.target, ethers.MaxUint256);
    }

    // A loan collateralised in the malicious token, taken and defaulted.
    const now = await time.latest();
    const decl = {
      token: evil.target, posKind: Pos.Categorical, legs: 2, scalarMin: 0, scalarMax: 0,
      resKind: Res.Deadline, deadline: now + 60, quorum: 0, refMarket: 0, refWhen: 0,
      payKind: Pay.Seizure, strike: 0, isCall: false, seizeTo: 1, discharge: 0, expiry: now + 30 * DAY,
    };
    const id = await mkts.connect(borrower).createMarket.staticCall(decl, [], "");
    await mkts.connect(borrower).createMarket(decl, [], "");
    const offerId = await mkts.connect(borrower).postOffer.staticCall(id, 0, 150_000, 0, 0);
    await mkts.connect(borrower).postOffer(id, 0, 150_000, 0, 0);
    await mkts.connect(lender).acceptOffer(offerId);

    await time.increase(61);
    await mkts.connect(watcher).settle(id);

    // Arm the token to reenter claim() during the payout, then claim once.
    await evil.connect(lender).arm(mkts.target, id);
    const owed = await mkts.claimable(id, lender.address);
    const before = await evil.balanceOf(lender.address);
    await mkts.connect(lender).claim(id);
    const gained = (await evil.balanceOf(lender.address)) - before;

    // The reentrant second claim inside the transfer must have taken nothing:
    // the lender is paid exactly once, and the contract keeps no phantom debt.
    expect(gained).to.equal(owed);
    expect(await evil.balanceOf(mkts.target)).to.equal(0n);
    await expect(mkts.connect(lender).claim(id)).to.be.revertedWith("nothing to claim here");
  });

  it("the contract never holds more or less than it owes, across a full story", async () => {
    // Conservation is the invariant an auditor checks first: for any token, the
    // contract's balance equals the sum of open escrows and unclaimed payouts,
    // and value is never minted or burned by the market logic.
    const usd = await ethers.deployContract("MeapUSD");
    for (const who of [borrower, lender, watcher]) {
      await usd.connect(who).faucet(1_000_000);
      await usd.connect(who).approve(mkts.target, ethers.MaxUint256);
    }
    const supply = 3n * 1_000_000n;
    const sumAll = async () => {
      let t = await usd.balanceOf(mkts.target);
      for (const w of [borrower, lender, watcher]) t += await usd.balanceOf(w.address);
      return t;
    };
    expect(await sumAll()).to.equal(supply);

    const now = await time.latest();
    const decl = {
      token: usd.target, posKind: Pos.Categorical, legs: 2, scalarMin: 0, scalarMax: 0,
      resKind: Res.Deadline, deadline: now + 60, quorum: 0, refMarket: 0, refWhen: 0,
      payKind: Pay.Seizure, strike: 0, isCall: false, seizeTo: 1, discharge: 110_000, expiry: now + 30 * DAY,
    };
    const id = await mkts.connect(borrower).createMarket.staticCall(decl, [], "");
    await mkts.connect(borrower).createMarket(decl, [], "");
    const offerId = await mkts.connect(borrower).postOffer.staticCall(id, 0, 150_000, 100_000, 0);
    await mkts.connect(borrower).postOffer(id, 0, 150_000, 100_000, 0);
    await mkts.connect(lender).acceptOffer(offerId);
    expect(await sumAll()).to.equal(supply);          // moving money never changes the total

    await time.increase(61);
    await mkts.connect(watcher).settle(id);
    await mkts.connect(lender).claim(id);
    expect(await sumAll()).to.equal(supply);          // and neither does settling it
    expect(await usd.balanceOf(mkts.target)).to.equal(0n);
  });

  it("a settled market cannot be settled again to double-pay a bounty", async () => {
    const usd = await ethers.deployContract("MeapUSD");
    for (const who of [borrower, lender, watcher]) {
      await usd.connect(who).faucet(1_000_000);
      await usd.connect(who).approve(mkts.target, ethers.MaxUint256);
    }
    const now = await time.latest();
    const decl = {
      token: usd.target, posKind: Pos.Categorical, legs: 2, scalarMin: 0, scalarMax: 0,
      resKind: Res.Deadline, deadline: now + 60, quorum: 0, refMarket: 0, refWhen: 0,
      payKind: Pay.Seizure, strike: 0, isCall: false, seizeTo: 1, discharge: 0, expiry: now + 30 * DAY,
    };
    const id = await mkts.connect(borrower).createMarket.staticCall(decl, [], "");
    await mkts.connect(borrower).createMarket(decl, [], "");
    const offerId = await mkts.connect(borrower).postOffer.staticCall(id, 0, 150_000, 0, 0);
    await mkts.connect(borrower).postOffer(id, 0, 150_000, 0, 0);
    await mkts.connect(lender).acceptOffer(offerId);
    await time.increase(61);

    await mkts.connect(watcher).settle(id);
    await expect(mkts.connect(watcher).settle(id)).to.be.revertedWith("market is not open");
  });
});
