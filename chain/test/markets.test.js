/**
 * The grammar's claims, re-proven against the contract.
 *
 * These are the same scenarios the off-chain ledger passes, because the port
 * is only correct if the two engines agree about what the five fields mean: a
 * loan defaults and a stranger forecloses it for the bounty, the same loan
 * repaid comes home, insurance reads another market's default, an option
 * settles on an attested median, an undecidable market unwinds at expiry.
 *
 * On top of that, the things only a contract has to survive: double claims,
 * claims by losers, residuals to the declarer and nobody else, and exact
 * conservation measured the only way that matters here, by the token balance
 * of the contract itself.
 */

const { expect } = require("chai");
const { ethers } = require("hardhat");
const { time } = require("@nomicfoundation/hardhat-network-helpers");

const DAY = 86_400;

// enums, mirrored
const Pos = { Binary: 0, Categorical: 1, Scalar: 2 };
const Res = { Deadline: 0, Attestation: 1, MarketRef: 2 };
const Pay = { WinnerTakeAll: 0, Linear: 1, Kinked: 2, Seizure: 3 };
const State = { Open: 0, Settled: 1, Defaulted: 2, Expired: 3 };
const When = { Settled: 0, Defaulted: 1, Expired: 2 };

describe("MeapMarkets", () => {
  let usd, mkts;
  let borrower, lender, insurer, watcher, oracle, oracle2, liar;

  /** A declaration with every field present; tests override what they mean. */
  function decl(over = {}) {
    return {
      token: usd.target,
      posKind: Pos.Binary,
      legs: 2,
      scalarMin: 0,
      scalarMax: 0,
      resKind: Res.Attestation,
      deadline: 0,
      quorum: 1,
      refMarket: 0,
      refWhen: When.Settled,
      payKind: Pay.WinnerTakeAll,
      strike: 0,
      isCall: false,
      seizeTo: 0,
      discharge: 0,
      expiry: 0,
      ...over,
    };
  }

  async function create(as, d, attestors = [], label = "") {
    const id = await mkts.connect(as).createMarket.staticCall(d, attestors, label);
    await mkts.connect(as).createMarket(d, attestors, label);
    return id;
  }

  /** The loan from the ledger tests: 150k collateral, 100k principal, 110k back. */
  async function openLoan(now) {
    const loan = await create(borrower, decl({
      posKind: Pos.Categorical, legs: 2,            // 0 REPAID, 1 DEFAULTED
      resKind: Res.Deadline, deadline: now + 30 * DAY,
      payKind: Pay.Seizure, seizeTo: 1, discharge: 110_000,
      quorum: 0,
      expiry: now + 31 * DAY,
    }), [], "borrow 100000 against 150000");

    const offerId = await mkts.connect(borrower).postOffer.staticCall(loan, 0, 150_000, 100_000, 0);
    await mkts.connect(borrower).postOffer(loan, 0, 150_000, 100_000, 0);
    await mkts.connect(lender).acceptOffer(offerId);
    return { loan, offerId };
  }

  const bal = (who) => usd.balanceOf(who.address ?? who);
  const held = () => usd.balanceOf(mkts.target);

  beforeEach(async () => {
    [borrower, lender, insurer, watcher, oracle, oracle2, liar] = await ethers.getSigners();
    usd = await ethers.deployContract("MeapUSD");
    mkts = await ethers.deployContract("MeapMarkets");
    for (const who of [borrower, lender, insurer, watcher, oracle, oracle2, liar]) {
      await usd.connect(who).faucet(1_000_000);
      await usd.connect(who).approve(mkts.target, ethers.MaxUint256);
    }
  });

  // --- the loan, both fates -------------------------------------------------

  it("a loan defaults and a stranger forecloses it for the bounty", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);

    // The principal moved directly; the collateral sits in the contract.
    expect(await bal(borrower)).to.equal(1_000_000 - 150_000 + 100_000);
    expect(await bal(lender)).to.equal(1_000_000 - 100_000);
    expect(await held()).to.equal(150_000);

    await expect(mkts.connect(watcher).settle(loan)).to.be.revertedWith(
      "not ready: the deadline has not passed and nothing was discharged");

    await time.increase(31 * DAY);
    await mkts.connect(watcher).settle(loan);

    const m = await mkts.markets(loan);
    expect(m.state).to.equal(State.Defaulted);
    // The stranger who lent nothing earns 25bp of the pot for closing it out.
    expect(await bal(watcher)).to.equal(1_000_000 + 375);

    // The lender pulls the collateral, less the bounty.
    await mkts.connect(lender).claim(loan);
    expect(await bal(lender)).to.equal(1_000_000 - 100_000 + 149_625);

    // The borrower holds the losing leg and gets nothing.
    await expect(mkts.connect(borrower).claim(loan)).to.be.revertedWith("nothing to claim here");
    expect(await held()).to.equal(0);
  });

  it("the same loan repaid in time comes home", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);

    await mkts.connect(borrower).repay(loan);
    expect(await held()).to.equal(150_000 + 110_000);

    await mkts.connect(watcher).settle(loan);
    expect((await mkts.markets(loan)).state).to.equal(State.Settled);

    // Collateral (less bounty) back to the borrower; the repayment to the lender.
    await mkts.connect(borrower).claim(loan);
    await mkts.connect(lender).claim(loan);
    expect(await bal(borrower)).to.equal(1_000_000 - 150_000 + 100_000 - 110_000 + 149_625);
    expect(await bal(lender)).to.equal(1_000_000 - 100_000 + 110_000);
    expect(await held()).to.equal(0);
  });

  it("a loan cannot be repaid after the deadline, only foreclosed", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);
    await time.increase(31 * DAY);
    await expect(mkts.connect(borrower).repay(loan)).to.be.revertedWith(
      "the deadline has passed; only foreclosure remains");
  });

  it("only a holder of the obliged leg can discharge", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);
    await expect(mkts.connect(watcher).repay(loan)).to.be.revertedWith(
      "only a holder of the obliged leg discharges");
  });

  // --- insurance: a market that reads another market -------------------------

  it("insurance written against a loan pays when it defaults", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);

    // A third party nobody asked writes cover on that loan defaulting.
    const cover = await create(insurer, decl({
      resKind: Res.MarketRef, refMarket: loan, refWhen: When.Defaulted,
      quorum: 0,
      expiry: now + 40 * DAY,
    }), [], "pays if the loan defaults");

    // Watcher buys YES (leg 0) from the insurer's offer of NO (leg 1).
    const offerId = await mkts.connect(insurer).postOffer.staticCall(cover, 1, 20_000, 0, 5_000);
    await mkts.connect(insurer).postOffer(cover, 1, 20_000, 0, 5_000);
    await mkts.connect(watcher).acceptOffer(offerId);

    // Cannot settle while the loan is open.
    await expect(mkts.connect(watcher).settle(cover)).to.be.revertedWith(
      "not ready: the referenced market has not settled");

    await time.increase(31 * DAY);
    await mkts.connect(watcher).settle(loan);
    await mkts.connect(insurer).settle(cover);

    const c = await mkts.markets(cover);
    expect(c.state).to.equal(State.Settled);
    expect(c.outcomeLeg).to.equal(0);   // the condition held

    // The cover pays its YES holder; the insurer who wrote it eats the loss.
    const before = await bal(watcher);
    await mkts.connect(watcher).claim(cover);
    expect((await bal(watcher)) - before).to.equal(24_938n); // 25k less the 62 bounty
    await expect(mkts.connect(insurer).claim(cover)).to.be.revertedWith("nothing to claim here");
  });

  it("insurance on a loan that was repaid pays nothing", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);
    const cover = await create(insurer, decl({
      resKind: Res.MarketRef, refMarket: loan, refWhen: When.Defaulted,
      quorum: 0, expiry: now + 40 * DAY,
    }));
    const offerId = await mkts.connect(insurer).postOffer.staticCall(cover, 1, 20_000, 0, 5_000);
    await mkts.connect(insurer).postOffer(cover, 1, 20_000, 0, 5_000);
    await mkts.connect(watcher).acceptOffer(offerId);

    await mkts.connect(borrower).repay(loan);
    await mkts.connect(watcher).settle(loan);
    await mkts.connect(watcher).settle(cover);

    expect((await mkts.markets(cover)).outcomeLeg).to.equal(1);
    await expect(mkts.connect(watcher).claim(cover)).to.be.revertedWith("nothing to claim here");
    await mkts.connect(insurer).claim(cover); // the writer keeps the pot
  });

  it("a market can only reference one that already exists, so cycles cannot be written", async () => {
    const now = await time.latest();
    await expect(create(insurer, decl({
      resKind: Res.MarketRef, refMarket: 999, quorum: 0, expiry: now + DAY,
    }))).to.be.revertedWith("referenced market does not exist");
  });

  // --- attestation ------------------------------------------------------------

  it("attestors who match the outcome are paid and the liar is not", async () => {
    const now = await time.latest();
    const market = await create(insurer, decl({
      quorum: 2, expiry: now + 10 * DAY,
    }), [oracle.address, oracle2.address, liar.address]);

    const offerId = await mkts.connect(insurer).postOffer.staticCall(market, 0, 40_000, 0, 40_000);
    await mkts.connect(insurer).postOffer(market, 0, 40_000, 0, 40_000);
    await mkts.connect(watcher).acceptOffer(offerId);

    await mkts.connect(oracle).attest(market, 0, 0);
    await expect(mkts.connect(watcher).settle(market)).to.be.revertedWith(
      "not ready: no outcome has reached quorum");

    await mkts.connect(liar).attest(market, 1, 0);
    await mkts.connect(oracle2).attest(market, 0, 0);
    await mkts.connect(watcher).settle(market);

    const m = await mkts.markets(market);
    expect(m.state).to.equal(State.Settled);
    expect(m.outcomeLeg).to.equal(0);
    expect(m.matchedCount).to.equal(2);

    // Telling the truth pays; the pool splits between the two who matched.
    const pool = m.attestPool;
    const before = await bal(oracle);
    await mkts.connect(oracle).claimAttestor(market);
    expect((await bal(oracle)) - before).to.equal(pool / 2n);
    await expect(mkts.connect(liar).claimAttestor(market)).to.be.revertedWith(
      "your report did not match the outcome");
    await expect(mkts.connect(oracle).claimAttestor(market)).to.be.revertedWith("already paid");
  });

  it("only named attestors may report, and only once", async () => {
    const now = await time.latest();
    const market = await create(insurer, decl({ expiry: now + DAY }), [oracle.address]);
    await expect(mkts.connect(watcher).attest(market, 0, 0)).to.be.revertedWith(
      "you are not an attestor on this market");
    await mkts.connect(oracle).attest(market, 0, 0);
    await expect(mkts.connect(oracle).attest(market, 1, 0)).to.be.revertedWith(
      "you have already attested; reports are final");
  });

  // --- the option -------------------------------------------------------------

  it("a call settles on the attested median and splits the pot by moneyness", async () => {
    const now = await time.latest();
    // Strike 200 on 0..400. Settles at 300: half in the money.
    const market = await create(borrower, decl({
      posKind: Pos.Scalar, scalarMin: 0, scalarMax: 400,
      payKind: Pay.Kinked, strike: 200, isCall: true,
      quorum: 3, expiry: now + DAY,
    }), [oracle.address, oracle2.address, liar.address], "call struck at 200");

    const offerId = await mkts.connect(borrower).postOffer.staticCall(market, 0, 50_000, 0, 50_000);
    await mkts.connect(borrower).postOffer(market, 0, 50_000, 0, 50_000);
    await mkts.connect(insurer).acceptOffer(offerId);

    await mkts.connect(oracle).attest(market, 0, 300);
    await mkts.connect(oracle2).attest(market, 0, 300);
    await mkts.connect(liar).attest(market, 0, 40_000);   // the outlier
    await mkts.connect(watcher).settle(market);

    const m = await mkts.markets(market);
    expect(m.outcomeValue).to.equal(300);   // the median shrugs off the outlier
    expect(m.matchedCount).to.equal(2);     // and the outlier earns nothing

    // LONG (borrower) and SHORT (insurer) split the net evenly at half ITM.
    const longSide = await mkts.claimable(market, borrower.address);
    const shortSide = await mkts.claimable(market, insurer.address);
    expect(longSide).to.equal(shortSide);
    await mkts.connect(borrower).claim(market);
    await mkts.connect(insurer).claim(market);
  });

  // --- expiry -----------------------------------------------------------------

  it("a market nobody can decide expires and returns the stakes, without a bounty", async () => {
    const now = await time.latest();
    const market = await create(insurer, decl({ expiry: now + DAY }), [oracle.address]);
    const offerId = await mkts.connect(borrower).postOffer.staticCall(market, 0, 20_000, 0, 20_000);
    await mkts.connect(borrower).postOffer(market, 0, 20_000, 0, 20_000);
    await mkts.connect(lender).acceptOffer(offerId);

    await time.increase(2 * DAY);
    const watcherBefore = await bal(watcher);
    await mkts.connect(watcher).settle(market);
    expect(await bal(watcher)).to.equal(watcherBefore);   // no bounty: nothing was decided

    expect((await mkts.markets(market)).state).to.equal(State.Expired);
    await mkts.connect(borrower).claim(market);
    await mkts.connect(lender).claim(market);
    expect(await bal(borrower)).to.equal(1_000_000);
    expect(await bal(lender)).to.equal(1_000_000);
    expect(await held()).to.equal(0);
  });

  it("an untaken offer's stake survives expiry, because it never joined the escrow", async () => {
    const now = await time.latest();
    const market = await create(insurer, decl({ expiry: now + DAY }), [oracle.address]);
    const offerId = await mkts.connect(borrower).postOffer.staticCall(market, 0, 20_000, 0, 0);
    await mkts.connect(borrower).postOffer(market, 0, 20_000, 0, 0);

    await time.increase(2 * DAY);
    await mkts.connect(watcher).settle(market);
    await mkts.connect(borrower).cancelOffer(offerId);
    expect(await bal(borrower)).to.equal(1_000_000);
  });

  // --- claims are airtight ----------------------------------------------------

  it("nobody claims twice, losers claim nothing, strangers claim nothing", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);
    await time.increase(31 * DAY);
    await mkts.connect(watcher).settle(loan);

    await mkts.connect(lender).claim(loan);
    await expect(mkts.connect(lender).claim(loan)).to.be.revertedWith("nothing to claim here");
    await expect(mkts.connect(borrower).claim(loan)).to.be.revertedWith("nothing to claim here");
    await expect(mkts.connect(watcher).claim(loan)).to.be.revertedWith("nothing to claim here");
  });

  it("a pot with nobody on the winning side goes to the declarer, never to the losers", async () => {
    const now = await time.latest();
    // A market where only NO was ever held: insurer posts NO and nobody takes
    // YES... which is impossible bilaterally, so build it via an offer where
    // the poster stakes everything and the taker stakes nothing, then have
    // the outcome land on a leg nobody holds. Simplest: categorical with 3
    // legs, positions only on legs 0 and 1, outcome attested to leg 2.
    const market = await create(insurer, decl({
      posKind: Pos.Categorical, legs: 3, quorum: 1, expiry: now + DAY,
    }), [oracle.address]);
    const offerId = await mkts.connect(borrower).postOffer.staticCall(market, 0, 10_000, 0, 10_000);
    await mkts.connect(borrower).postOffer(market, 0, 10_000, 0, 10_000);
    await mkts.connect(lender).acceptOffer(offerId);

    await mkts.connect(oracle).attest(market, 2, 0);
    await mkts.connect(watcher).settle(market);

    await expect(mkts.connect(borrower).claim(market)).to.be.revertedWith("nothing to claim here");
    await expect(mkts.connect(watcher).claimResidual(market)).to.be.revertedWith("only the declarer");
    const before = await bal(insurer);
    await mkts.connect(insurer).claimResidual(market);
    expect((await bal(insurer)) - before).to.equal((await mkts.markets(market)).net);
    await expect(mkts.connect(insurer).claimResidual(market)).to.be.revertedWith("already claimed");
  });

  // --- offers -----------------------------------------------------------------

  it("an offer cannot be taken by its poster, taken twice, or cancelled by a stranger", async () => {
    const now = await time.latest();
    const market = await create(insurer, decl({ expiry: now + DAY }), [oracle.address]);
    const offerId = await mkts.connect(borrower).postOffer.staticCall(market, 0, 10_000, 0, 0);
    await mkts.connect(borrower).postOffer(market, 0, 10_000, 0, 0);

    await expect(mkts.connect(borrower).acceptOffer(offerId)).to.be.revertedWith("cannot take your own offer");
    await expect(mkts.connect(watcher).cancelOffer(offerId)).to.be.revertedWith("only the poster cancels");
    await mkts.connect(lender).acceptOffer(offerId);
    await expect(mkts.connect(watcher).acceptOffer(offerId)).to.be.revertedWith("offer is not open");
    await expect(mkts.connect(borrower).cancelOffer(offerId)).to.be.revertedWith("offer is not open");
  });

  it("an offer with nothing at stake on either side is refused", async () => {
    const now = await time.latest();
    const market = await create(insurer, decl({ expiry: now + DAY }), [oracle.address]);
    await expect(mkts.connect(borrower).postOffer(market, 0, 0, 5_000, 0)).to.be.revertedWith(
      "an offer with nothing at stake settles nothing");
  });

  // --- the grammar still refuses nonsense --------------------------------------

  it("incoherent declarations are refused at creation", async () => {
    const now = await time.latest();
    const cases = [
      [decl({ payKind: Pay.Linear, expiry: now + DAY }), [], "linear and kinked require scalar positions"],
      [decl({ posKind: Pos.Scalar, scalarMin: 0, scalarMax: 10, payKind: Pay.WinnerTakeAll, expiry: now + DAY }),
        [oracle.address], "winner take all requires binary or categorical"],
      [decl({ posKind: Pos.Scalar, scalarMin: 0, scalarMax: 10, payKind: Pay.Kinked, strike: 99, expiry: now + DAY }),
        [oracle.address], "strike must lie inside the range"],
      [decl({ posKind: Pos.Scalar, scalarMin: 0, scalarMax: 10, payKind: Pay.Kinked, strike: 10, isCall: true, expiry: now + DAY }),
        [oracle.address], "a call struck at the top can never pay"],
      [decl({ payKind: Pay.Seizure, seizeTo: 1, resKind: Res.Attestation, expiry: now + DAY }),
        [oracle.address], "seizure resolves by deadline"],
      [decl({ resKind: Res.Deadline, payKind: Pay.WinnerTakeAll, deadline: now + DAY, quorum: 0, expiry: now + 2 * DAY }),
        [], "a deadline only resolves a seizure"],
      [decl({ resKind: Res.Deadline, payKind: Pay.Seizure, seizeTo: 1, deadline: now + 9 * DAY, quorum: 0, expiry: now + DAY }),
        [], "deadline falls after expiry"],
      [decl({ expiry: now - 1 }), [oracle.address], "expiry is already in the past"],
      [decl({ quorum: 5, expiry: now + DAY }), [oracle.address], "quorum in 1..attestors"],
      [decl({ expiry: now + DAY }), [oracle.address, oracle.address], "attestors must be distinct"],
      [decl({ posKind: Pos.Categorical, legs: 9, expiry: now + DAY }), [oracle.address], "categorical needs 2..8 legs"],
    ];
    for (const [d, atts, msg] of cases) {
      await expect(create(borrower, d, atts)).to.be.revertedWith(msg);
    }
  });

  // --- conservation, the only way that matters on chain -------------------------

  it("the contract's token balance is exactly the sum of what is still owed", async () => {
    const now = await time.latest();
    const { loan } = await openLoan(now);
    const cover = await create(insurer, decl({
      resKind: Res.MarketRef, refMarket: loan, refWhen: When.Defaulted,
      quorum: 0, expiry: now + 40 * DAY,
    }));
    const offerId = await mkts.connect(insurer).postOffer.staticCall(cover, 1, 20_000, 0, 5_000);
    await mkts.connect(insurer).postOffer(cover, 1, 20_000, 0, 5_000);
    await mkts.connect(watcher).acceptOffer(offerId);

    expect(await held()).to.equal(150_000 + 25_000);

    await time.increase(31 * DAY);
    await mkts.connect(watcher).settle(loan);
    await mkts.connect(watcher).settle(cover);
    await mkts.connect(lender).claim(loan);
    await mkts.connect(watcher).claim(cover);

    // Everything that could be claimed has been; only rounding dust may stay.
    expect(await held()).to.be.lessThan(3);

    // And no token was created or destroyed across the whole story.
    let total = 0n;
    for (const who of [borrower, lender, insurer, watcher, oracle, oracle2, liar]) {
      total += await bal(who);
    }
    total += await held();
    expect(total).to.equal(7n * 1_000_000n);
  });
});
