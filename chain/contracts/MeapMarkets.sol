// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/**
 * MEAP's market grammar, enforced by contract instead of by a server.
 *
 * This is a port of mcp/src/{grammar,ledger,settle}.js with the trust removed:
 * the off-chain ledger asks to be believed (and proves itself via a replayable
 * log), while this cannot lie about custody at all, because the collateral
 * sits here and the payoff rules are the bytecode. The operator disappears
 * from the trust model, which is what "MEAP does not custody" ultimately
 * has to mean.
 *
 * A market is still declared, never coded. The declaration struct carries the
 * same five fields, the engine interprets them, and the conventional
 * instruments remain absent from the code: a loan is a deadline plus a seizure
 * payoff, insurance is a market resolving on another market's default, an
 * option is scalar positions with a kinked payoff. Everything the grammar
 * refuses off-chain is refused here at declaration.
 *
 * Two things changed shape in the port, both forced by gas:
 *
 * Payouts are pulled, not pushed. The ledger iterates every holder at
 * settlement; a contract cannot loop over an unbounded set, so settle() fixes
 * the outcome and pays the settler's bounty, and each holder claims their own
 * share afterwards. Same economics, different direction.
 *
 * Recursion became free. Off-chain, a market resolving on another market needs
 * a cycle check and a depth cap. Here a declaration may only reference a
 * market that already exists, so references point strictly backwards in time
 * and no cycle can be written at all.
 *
 * One rule tightened: seizure and deadline now imply each other. The grammar
 * technically allowed a seizure settled by attestation, but repay() reads the
 * deadline, so the combination was a latent trap. Here it cannot be declared.
 *
 * Mechanism is bilateral only. LMSR needs fixed-point exp/ln and is the one
 * part of the grammar that does not port in an afternoon; it is absent rather
 * than approximated, for the same reason the vocabulary has never listed a
 * verb the engine cannot run.
 */
contract MeapMarkets is ReentrancyGuard {
    using SafeERC20 for IERC20;

    enum PosKind { Binary, Categorical, Scalar }
    enum ResKind { Deadline, Attestation, MarketRef }
    enum PayKind { WinnerTakeAll, Linear, Kinked, Seizure }
    enum State { Open, Settled, Defaulted, Expired }
    enum RefWhen { Settled, Defaulted, Expired }

    /// Taken from escrow at settlement, in basis points. Same numbers as the
    /// off-chain ledger: closing out a ready market is paid work open to
    /// anyone, and attestors who matched the outcome share a slice.
    uint256 public constant BOUNTY_BPS = 25;
    uint256 public constant ATTEST_BPS = 50;
    uint256 public constant MAX_ATTESTORS = 16;
    uint256 public constant MAX_LEGS = 8;

    struct Declaration {
        IERC20 token;        // collateral: what backs it
        PosKind posKind;     // positions: what can be held
        uint8 legs;          // binary 2, categorical 2..8, scalar 2 (0 LONG, 1 SHORT)
        int64 scalarMin;
        int64 scalarMax;
        ResKind resKind;     // resolution: where truth arrives from
        uint64 deadline;     // Deadline only: when the obligation falls due
        uint8 quorum;        // Attestation only
        uint64 refMarket;    // MarketRef only: an EARLIER market's id
        RefWhen refWhen;
        PayKind payKind;     // payoff: how escrow maps to legs at settlement
        int64 strike;        // Kinked only
        bool isCall;
        uint8 seizeTo;       // Seizure only: the leg that takes the pot on default
        uint128 discharge;   // Seizure only: what settles the obligation early
        uint64 expiry;       // after this an undecidable market unwinds
    }

    struct Market {
        address declarer;
        Declaration decl;
        State state;
        bool discharged;
        uint8 outcomeLeg;
        int64 outcomeValue;
        uint128 escrow;         // accepted stakes; offer stakes stay with their offer
        uint128 dischargePool;  // a repayment, waiting for the seizing leg to claim it
        uint128 net;            // escrow after bounty and attestor pool, set at settle
        uint128 attestPool;
        uint16 matchedCount;
        bool residualClaimed;
    }

    struct Offer {
        uint64 market;
        address poster;
        uint8 leg;
        uint128 stake;          // escrowed by the poster on posting
        uint128 ask;            // paid poster-ward by the taker, directly
        uint128 counterStake;   // escrowed by the taker on acceptance
        uint8 status;           // 0 open, 1 taken, 2 cancelled
    }

    struct Attestation {
        bool done;
        bool matched;
        bool paid;
        uint8 leg;
        int64 value;
    }

    uint64 public nextMarket = 1;
    uint64 public nextOffer = 1;

    mapping(uint64 => Market) public markets;
    mapping(uint64 => address[]) internal _attestors;
    mapping(uint64 => Offer) public offers;
    mapping(uint64 => mapping(address => mapping(uint8 => uint128))) public holdings;
    mapping(uint64 => mapping(uint8 => uint128)) public legTotal;
    mapping(uint64 => mapping(address => Attestation)) public attestations;
    mapping(uint64 => mapping(address => bool)) public claimed;

    /// The label rides in the event rather than in storage: it is free text
    /// written by one agent and read by others, displayed but never
    /// interpreted, and events are where display-only data belongs.
    event MarketCreated(uint64 indexed id, address indexed declarer, string label);
    event OfferPosted(uint64 indexed id, uint64 indexed market, address indexed poster, uint8 leg, uint128 stake, uint128 ask, uint128 counterStake);
    event OfferCancelled(uint64 indexed id);
    event OfferTaken(uint64 indexed id, uint64 indexed market, address indexed taker, uint128 shares);
    event Repaid(uint64 indexed market, address indexed by, uint128 amount);
    event Attested(uint64 indexed market, address indexed by);
    event Settled(uint64 indexed market, State state, uint8 outcomeLeg, int64 outcomeValue, address indexed settler, uint256 bounty);
    event Claimed(uint64 indexed market, address indexed by, uint256 amount);

    // --- declaring ----------------------------------------------------------

    function createMarket(
        Declaration calldata d,
        address[] calldata attestors_,
        string calldata label
    ) external returns (uint64 id) {
        require(address(d.token) != address(0), "collateral needs a token");
        require(d.expiry > block.timestamp, "expiry is already in the past");

        // positions
        if (d.posKind == PosKind.Binary) {
            require(d.legs == 2, "binary has exactly two legs");
        } else if (d.posKind == PosKind.Categorical) {
            require(d.legs >= 2 && d.legs <= MAX_LEGS, "categorical needs 2..8 legs");
        } else {
            require(d.legs == 2, "scalar has exactly two legs");
            require(d.scalarMax > d.scalarMin, "scalar needs max > min");
        }

        // payoff coherence, as in the grammar: combinations that mean nothing
        // together are refused at declaration, not discovered at settlement.
        if (d.payKind == PayKind.Linear || d.payKind == PayKind.Kinked) {
            require(d.posKind == PosKind.Scalar, "linear and kinked require scalar positions");
            require(d.resKind == ResKind.Attestation, "a scalar payoff needs an attested value");
        }
        if (d.payKind == PayKind.Kinked) {
            require(d.strike >= d.scalarMin && d.strike <= d.scalarMax, "strike must lie inside the range");
            if (d.isCall) require(d.strike < d.scalarMax, "a call struck at the top can never pay");
            else require(d.strike > d.scalarMin, "a put struck at the bottom can never pay");
        }
        if (d.payKind == PayKind.WinnerTakeAll) {
            require(d.posKind != PosKind.Scalar, "winner take all requires binary or categorical");
        }
        if (d.payKind == PayKind.Seizure) {
            require(d.legs == 2, "seizure needs exactly two legs");
            require(d.seizeTo < 2, "seizeTo names a leg");
            require(d.resKind == ResKind.Deadline, "seizure resolves by deadline");
        }

        // resolution
        if (d.resKind == ResKind.Deadline) {
            require(d.payKind == PayKind.Seizure, "a deadline only resolves a seizure");
            require(d.deadline > block.timestamp, "deadline is already in the past");
            require(d.deadline <= d.expiry, "deadline falls after expiry");
            require(attestors_.length == 0, "deadline takes no attestors");
        } else if (d.resKind == ResKind.Attestation) {
            require(attestors_.length >= 1 && attestors_.length <= MAX_ATTESTORS, "1..16 attestors");
            require(d.quorum >= 1 && d.quorum <= attestors_.length, "quorum in 1..attestors");
            for (uint256 i = 0; i < attestors_.length; i++) {
                require(attestors_[i] != address(0), "attestor cannot be zero");
                for (uint256 j = i + 1; j < attestors_.length; j++) {
                    require(attestors_[i] != attestors_[j], "attestors must be distinct");
                }
            }
        } else {
            // The recursive case, and the reason no cycle check exists here: a
            // declaration may only reference a market that already exists, so
            // the reference graph points strictly backwards in time.
            require(d.refMarket >= 1 && d.refMarket < nextMarket, "referenced market does not exist");
            require(d.legs == 2, "a market reading another needs two legs: held or not");
            require(attestors_.length == 0, "market reference takes no attestors");
        }

        id = nextMarket++;
        Market storage m = markets[id];
        m.declarer = msg.sender;
        m.decl = d;
        for (uint256 i = 0; i < attestors_.length; i++) _attestors[id].push(attestors_[i]);
        emit MarketCreated(id, msg.sender, label);
    }

    function attestorsOf(uint64 id) external view returns (address[] memory) {
        return _attestors[id];
    }

    // --- taking a side ------------------------------------------------------

    /// Offer one side. The stake is escrowed with the offer immediately, so an
    /// offer is always backed; it joins the market's escrow only when taken,
    /// which is what keeps an expired market's unwind arithmetic honest.
    function postOffer(uint64 market, uint8 leg, uint128 stake, uint128 ask, uint128 counterStake)
        external nonReentrant returns (uint64 id)
    {
        Market storage m = markets[market];
        require(m.declarer != address(0), "unknown market");
        require(m.state == State.Open, "market is not open");
        require(leg < m.decl.legs, "no such leg");
        require(stake > 0 || counterStake > 0, "an offer with nothing at stake settles nothing");

        if (stake > 0) m.decl.token.safeTransferFrom(msg.sender, address(this), stake);

        id = nextOffer++;
        offers[id] = Offer(market, msg.sender, leg, stake, ask, counterStake, 0);
        emit OfferPosted(id, market, msg.sender, leg, stake, ask, counterStake);
    }

    /// Cancellation works in any market state: an untaken stake belongs to its
    /// poster, and an expired market must not strand it.
    function cancelOffer(uint64 id) external nonReentrant {
        Offer storage o = offers[id];
        require(o.poster == msg.sender, "only the poster cancels");
        require(o.status == 0, "offer is not open");
        o.status = 2;
        if (o.stake > 0) markets[o.market].decl.token.safeTransfer(msg.sender, o.stake);
        emit OfferCancelled(id);
    }

    /// Both sides receive shares equal to the pair's total contribution, so a
    /// lender who escrows nothing still holds the full claim on the collateral:
    /// the claim is on the pot, and the two legs claim it from opposite sides.
    function acceptOffer(uint64 id) external nonReentrant {
        Offer storage o = offers[id];
        require(o.poster != address(0), "unknown offer");
        require(o.status == 0, "offer is not open");
        require(o.poster != msg.sender, "cannot take your own offer");

        Market storage m = markets[o.market];
        require(m.state == State.Open, "market is not open");

        o.status = 1;
        if (o.ask > 0) m.decl.token.safeTransferFrom(msg.sender, o.poster, o.ask);
        if (o.counterStake > 0) m.decl.token.safeTransferFrom(msg.sender, address(this), o.counterStake);

        uint128 shares = o.stake + o.counterStake;
        uint8 takerLeg = o.leg == 0 ? 1 : 0;
        m.escrow += shares;
        holdings[o.market][o.poster][o.leg] += shares;
        holdings[o.market][msg.sender][takerLeg] += shares;
        legTotal[o.market][o.leg] += shares;
        legTotal[o.market][takerLeg] += shares;
        emit OfferTaken(id, o.market, msg.sender, shares);
    }

    // --- obligations --------------------------------------------------------

    /// Discharge before the deadline, so the collateral comes home instead of
    /// being seized. Any holder of the obliged leg may pay: a third party can
    /// rescue a position it did not open. The payment waits in its own pool
    /// for the seizing leg to claim at settlement.
    function repay(uint64 market) external nonReentrant {
        Market storage m = markets[market];
        require(m.state == State.Open, "market is not open");
        require(m.decl.payKind == PayKind.Seizure, "nothing to discharge here");
        require(!m.discharged, "already discharged");
        require(block.timestamp < m.decl.deadline, "the deadline has passed; only foreclosure remains");
        require(m.decl.discharge > 0, "this market declares no discharge; it can only be seized");

        uint8 obliged = m.decl.seizeTo == 0 ? 1 : 0;
        require(holdings[market][msg.sender][obliged] > 0, "only a holder of the obliged leg discharges");

        m.discharged = true;
        m.dischargePool = m.decl.discharge;
        m.decl.token.safeTransferFrom(msg.sender, address(this), m.decl.discharge);
        emit Repaid(market, msg.sender, m.decl.discharge);
    }

    /// Report what happened. Named attestors only, one report each, final.
    /// Settlement pays the ones who matched the outcome and nothing to the
    /// rest, which is the entire wage structure of truth telling here.
    function attest(uint64 market, uint8 leg, int64 value) external {
        Market storage m = markets[market];
        require(m.state == State.Open, "market is not open");
        require(m.decl.resKind == ResKind.Attestation, "this market does not take attestations");

        bool named = false;
        address[] storage atts = _attestors[market];
        for (uint256 i = 0; i < atts.length; i++) {
            if (atts[i] == msg.sender) { named = true; break; }
        }
        require(named, "you are not an attestor on this market");
        Attestation storage a = attestations[market][msg.sender];
        require(!a.done, "you have already attested; reports are final");

        if (m.decl.posKind == PosKind.Scalar) {
            a.value = value;
        } else {
            require(leg < m.decl.legs, "no such leg");
            a.leg = leg;
        }
        a.done = true;
        emit Attested(market, msg.sender);
    }

    // --- settlement ---------------------------------------------------------

    /// Close out a market that is ready, and take the bounty for doing it.
    /// Open to anyone: watching for obligations that have come due is a
    /// living, not a privilege of the lender. A market that cannot be decided
    /// and is past its expiry unwinds instead, with no bounty, because nothing
    /// was decided.
    function settle(uint64 market) external nonReentrant {
        Market storage m = markets[market];
        require(m.declarer != address(0), "unknown market");
        require(m.state == State.Open, "market is not open");

        (bool ready, State newState, uint8 outLeg, int64 outValue, string memory why) = _resolve(market, m);

        if (!ready) {
            if (block.timestamp >= m.decl.expiry) {
                m.state = State.Expired;
                emit Settled(market, State.Expired, 0, 0, msg.sender, 0);
                return;
            }
            revert(why);
        }

        uint256 bounty = (uint256(m.escrow) * BOUNTY_BPS) / 10_000;

        uint256 pool = 0;
        uint16 matchedCount = 0;
        if (m.decl.resKind == ResKind.Attestation) {
            address[] storage atts = _attestors[market];
            for (uint256 i = 0; i < atts.length; i++) {
                Attestation storage a = attestations[market][atts[i]];
                if (!a.done) continue;
                bool ok;
                if (m.decl.posKind == PosKind.Scalar) {
                    // Within one percent of the range of the settled value: an
                    // attestor who reported an outlier moved nothing and earns
                    // nothing.
                    int256 diff = int256(a.value) - int256(outValue);
                    if (diff < 0) diff = -diff;
                    ok = diff * 100 <= int256(m.decl.scalarMax) - int256(m.decl.scalarMin);
                } else {
                    ok = a.leg == outLeg;
                }
                if (ok) { a.matched = true; matchedCount++; }
            }
            if (matchedCount > 0) pool = (uint256(m.escrow) * ATTEST_BPS) / 10_000;
        }

        m.state = newState;
        m.outcomeLeg = outLeg;
        m.outcomeValue = outValue;
        m.attestPool = uint128(pool);
        m.matchedCount = matchedCount;
        m.net = uint128(uint256(m.escrow) - bounty - pool);

        emit Settled(market, newState, outLeg, outValue, msg.sender, bounty);
        if (bounty > 0) m.decl.token.safeTransfer(msg.sender, bounty);
    }

    function _resolve(uint64 id, Market storage m)
        internal view
        returns (bool ready, State newState, uint8 outLeg, int64 outValue, string memory why)
    {
        Declaration storage d = m.decl;

        if (d.resKind == ResKind.Deadline) {
            uint8 obliged = d.seizeTo == 0 ? 1 : 0;
            if (m.discharged) return (true, State.Settled, obliged, 0, "");
            if (block.timestamp >= d.deadline) return (true, State.Defaulted, d.seizeTo, 0, "");
            return (false, State.Open, 0, 0, "not ready: the deadline has not passed and nothing was discharged");
        }

        if (d.resKind == ResKind.Attestation) {
            address[] storage atts = _attestors[id];
            if (d.posKind == PosKind.Scalar) {
                int64[] memory vals = new int64[](atts.length);
                uint256 n = 0;
                for (uint256 i = 0; i < atts.length; i++) {
                    Attestation storage a = attestations[id][atts[i]];
                    if (a.done) vals[n++] = a.value;
                }
                if (n < d.quorum) return (false, State.Open, 0, 0, "not ready: quorum of attestations not reached");
                // Median of what was reported, so one liar cannot move it.
                for (uint256 i = 1; i < n; i++) {
                    int64 key = vals[i];
                    uint256 j = i;
                    while (j > 0 && vals[j - 1] > key) { vals[j] = vals[j - 1]; j--; }
                    vals[j] = key;
                }
                int64 med = n % 2 == 1 ? vals[n / 2] : int64((int256(vals[n / 2 - 1]) + int256(vals[n / 2])) / 2);
                return (true, State.Settled, 0, med, "");
            }
            uint256[] memory tally = new uint256[](d.legs);
            for (uint256 i = 0; i < atts.length; i++) {
                Attestation storage a = attestations[id][atts[i]];
                if (a.done) tally[a.leg]++;
            }
            for (uint8 leg = 0; leg < d.legs; leg++) {
                if (tally[leg] >= d.quorum) return (true, State.Settled, leg, 0, "");
            }
            return (false, State.Open, 0, 0, "not ready: no outcome has reached quorum");
        }

        // MarketRef: read the earlier market's terminal state. Not an error
        // while it is still open, simply not ready; this is how a chain of
        // dependent instruments unwinds in order.
        Market storage ref = markets[d.refMarket];
        if (ref.state == State.Open) return (false, State.Open, 0, 0, "not ready: the referenced market has not settled");
        State want = d.refWhen == RefWhen.Settled ? State.Settled
            : d.refWhen == RefWhen.Defaulted ? State.Defaulted : State.Expired;
        return (true, State.Settled, ref.state == want ? 0 : 1, 0, "");
    }

    // --- claims -------------------------------------------------------------

    /// What `who` could take home right now. Pure arithmetic over fixed state,
    /// so an agent can ask before it acts, which was the point of settlement
    /// being separable off-chain too.
    function claimable(uint64 market, address who) public view returns (uint256 amount) {
        Market storage m = markets[market];
        if (m.state == State.Open) return 0;
        if (claimed[market][who]) return 0;
        Declaration storage d = m.decl;

        if (m.state == State.Expired) {
            // Undecided: stakes come back in proportion to everything held,
            // with no bounty, because nothing was decided.
            uint256 all;
            uint256 mine;
            for (uint8 leg = 0; leg < d.legs; leg++) {
                all += legTotal[market][leg];
                mine += holdings[market][who][leg];
            }
            if (all == 0) return 0;
            return (uint256(m.escrow) * mine) / all;
        }

        if (d.payKind == PayKind.Seizure) {
            uint8 obliged = d.seizeTo == 0 ? 1 : 0;
            if (m.state == State.Defaulted) {
                return _share(m.net, holdings[market][who][d.seizeTo], legTotal[market][d.seizeTo]);
            }
            // Discharged: the collateral comes home, and the seizing leg takes
            // the discharge payment instead of the pot.
            return _share(m.net, holdings[market][who][obliged], legTotal[market][obliged])
                + _share(m.dischargePool, holdings[market][who][d.seizeTo], legTotal[market][d.seizeTo]);
        }

        if (d.payKind == PayKind.WinnerTakeAll) {
            return _share(m.net, holdings[market][who][m.outcomeLeg], legTotal[market][m.outcomeLeg]);
        }

        // Linear or kinked: the settled value fixes how the pot divides
        // between LONG (leg 0) and SHORT (leg 1).
        (uint256 longPool, uint256 shortPool) = _scalarPools(m);
        return _share(longPool, holdings[market][who][0], legTotal[market][0])
            + _share(shortPool, holdings[market][who][1], legTotal[market][1]);
    }

    function claim(uint64 market) external nonReentrant {
        uint256 amount = claimable(market, msg.sender);
        require(amount > 0, "nothing to claim here");
        claimed[market][msg.sender] = true;
        markets[market].decl.token.safeTransfer(msg.sender, amount);
        emit Claimed(market, msg.sender, amount);
    }

    /// A pool with nobody on its winning side goes to the declarer, who is the
    /// residual claimant, never back to the losers. Same rule as off-chain.
    function claimResidual(uint64 market) external nonReentrant {
        Market storage m = markets[market];
        require(msg.sender == m.declarer, "only the declarer");
        require(m.state != State.Open, "market is not settled");
        require(!m.residualClaimed, "already claimed");

        uint256 amount = 0;
        Declaration storage d = m.decl;
        if (m.state == State.Expired) {
            uint256 all;
            for (uint8 leg = 0; leg < d.legs; leg++) all += legTotal[market][leg];
            if (all == 0) amount = m.escrow;
        } else if (d.payKind == PayKind.Seizure) {
            uint8 obliged = d.seizeTo == 0 ? 1 : 0;
            if (m.state == State.Defaulted) {
                if (legTotal[market][d.seizeTo] == 0) amount = m.net;
            } else {
                if (legTotal[market][obliged] == 0) amount += m.net;
                if (legTotal[market][d.seizeTo] == 0) amount += m.dischargePool;
            }
        } else if (d.payKind == PayKind.WinnerTakeAll) {
            if (legTotal[market][m.outcomeLeg] == 0) amount = m.net;
        } else {
            (uint256 longPool, uint256 shortPool) = _scalarPools(m);
            if (legTotal[market][0] == 0) amount += longPool;
            if (legTotal[market][1] == 0) amount += shortPool;
        }

        require(amount > 0, "no residual here");
        m.residualClaimed = true;
        d.token.safeTransfer(msg.sender, amount);
        emit Claimed(market, msg.sender, amount);
    }

    /// The wage for having told the truth: an equal split of the attestor pool
    /// among reports that matched the outcome. Mismatched reports get nothing,
    /// which is the only thing that makes an attestation worth anything.
    function claimAttestor(uint64 market) external nonReentrant {
        Market storage m = markets[market];
        require(m.state == State.Settled, "market is not settled");
        Attestation storage a = attestations[market][msg.sender];
        require(a.matched, "your report did not match the outcome");
        require(!a.paid, "already paid");
        a.paid = true;
        uint256 amount = uint256(m.attestPool) / m.matchedCount;
        m.decl.token.safeTransfer(msg.sender, amount);
        emit Claimed(market, msg.sender, amount);
    }

    // --- internals ----------------------------------------------------------

    function _share(uint256 pool, uint256 mine, uint256 total) internal pure returns (uint256) {
        if (total == 0 || mine == 0) return 0;
        return (pool * mine) / total;
    }

    function _scalarPools(Market storage m) internal view returns (uint256 longPool, uint256 shortPool) {
        Declaration storage d = m.decl;
        int256 v = int256(m.outcomeValue);
        if (v < d.scalarMin) v = d.scalarMin;
        if (v > d.scalarMax) v = d.scalarMax;

        uint256 num;
        uint256 den;
        if (d.payKind == PayKind.Linear) {
            num = uint256(v - d.scalarMin);
            den = uint256(int256(d.scalarMax) - int256(d.scalarMin));
        } else if (d.isCall) {
            num = v > d.strike ? uint256(v - d.strike) : 0;
            den = uint256(int256(d.scalarMax) - int256(d.strike));
        } else {
            num = v < d.strike ? uint256(int256(d.strike) - v) : 0;
            den = uint256(int256(d.strike) - int256(d.scalarMin));
        }
        longPool = (uint256(m.net) * num) / den;
        shortPool = uint256(m.net) - longPool;
    }
}
