# 🎟️ Seat Pool

**Don't buy a seat. Pool for one.**

Seat Pool is a NEAR smart contract for group ticket buying: buyers pool funds into escrow, sellers bid **blindly** (commit–reveal), and `finalize` mints non-transferable seat passes at the **median cap** of revealed bids. One greedy bidder can't gouge the pool, and every dispute path refunds everyone in full.

**Live demo (read-only, calls the testnet chain directly):**
https://seatpool-demo-gilt.vercel.app

**Contract:** `pool2.spmw9b03jw.testnet` (NEAR testnet)

## Why

Flash-sale seat drops fail everyone: honest buyers lose to bots, sellers price blind, and escrow is a black box. Seat Pool turns the drop into a single audited auction — money sits in escrow, prices are cryptographically committed before reveal, settlement is deterministic.

## How a round runs

```
open_round → declare_inventory → join (escrow N) → bid (hash price + bond)
→ deadline → reveal → finalize = median cap, SEATPASS minted, escrow settled
```

1. **Host opens a round** — deadline + bid bond fixed on-chain.
2. **Seller declares inventory** — exact seat count, before money arrives.
3. **Buyers join** — flat contribution into escrow; join-or-refund, never stuck.
4. **Blind bids** — sellers commit `sha256(price|salt)` + bond; prices stay secret until reveal.
5. **Deadline → reveal** — late/fake reveals forfeit the bond.
6. **Finalize** — median of revealed prices becomes the cap; seat passes mint to buyers; escrow settles. Any dispute → full refund, nothing mints.

## Anti-gaming properties

| Attack | Why it fails |
|---|---|
| Price gouging via one high bid | Median, not max — a lone bidder can't move it |
| Peeking at rival bids | Commit–reveal: hashes lock in before the deadline |
| Reveal-withholding | Bond forfeited after deadline |
| Stuck funds | Every failure path is an explicit full refund |
| Pass resale panic | Seat passes are non-transferable |

## Run the demo yourself

```bash
npm install
# put a funded testnet key at keys/root3.json (gitignored)
node demo.mjs        # ~2.5 min full round on NEAR testnet
```

The contract is plain Rust ([contract/lib.rs](contract/lib.rs)); the demo drives it through JSON-RPC ([src/actor.mjs](src/actor.mjs)) — no SDK magic, everything you see is raw chain state.

## Status

- ✅ Rust contract (near-sdk 5.x) deployed to testnet
- ✅ End-to-end auction round live (escrow → blind bids → median settle → NFT mint)
- ✅ Public read-only dashboard (the Vercel link above)
- ⏳ Mainnet after audit

Built for the NEAR AI Hacks hackathon. Testnet N only — no real money moves.
