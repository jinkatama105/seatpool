# SEAT POOL — AGENT SKILL (paste this to your AI agent)

You are an autonomous NEAR testnet agent. You will buy pooled GPU/compute
seats at the Seat Pool contract. No registration, no API key — you only need
a NEAR testnet keypair that can sign transactions.

## The pool
- Contract account (the "host's pool"): `pool2.spmw9b03jw.testnet`
- RPC read: `https://near-testnet.drpc.org` (JSON-RPC 2.0, POST)
- TX push: `https://rpc.testnet.near.org` (broadcast_tx_commit)
- Explorer: `https://testnet.explorer.near.org/account/pool2.spmw9b03jw.testnet`

## Get a wallet (one time, ~2 min)
1. Create a free testnet account at https://wallet.testnet.near.org (faucet
   gives ~2 testnet N), or generate an ed25519 keypair and fund a sub-account.
2. Export the secret key (`ed25519:...`). Store it like a password.
   Your agent signs txs locally — the key never leaves your machine.

## Read live state (no key needed)
POST `{"jsonrpc":"2.0","id":"1","method":"query","params":{"request_type":
"call_function","finality":"final","account_id":"pool2.spmw9b03jw.testnet",
"method_name":"<fn>","args_base64":"<base64 of JSON args>"}}`

- `round_summary {"round":N}` -> `[buyers, revealed_bids, seats_total, closed]`
  (result is JSON bytes; round ids start at 0 — scan upward until `E32: no round`)
- `round_deadline {"round":N}` -> deadline in **nanoseconds**
- Joinable round = the highest id where `deadline_ns > chain now` (get `now`
  from `method:"block", params:{finality:"final"}` -> `header.timestamp_nanosec`).

## Join a round as a buyer/agent  (the only call YOU must make)
Function call on the pool account:

    join { "round": <id> }   with attached deposit = your budget (NEAR)

- deposit >= 0.1 N recommended (gas ~0.01 N, escrow is your budget).
- 1 join per account per round (dup join reverts E35).
- You can only join while `now < deadline_ns` of the open round.
- After finalize: cheapest seats sold to you at median cap, overpayment is
  auto-refunded to your wallet, and a non-transferable seat pass NFT is
  minted to you.

Proven live: tx `VnjRlauWzwFoAaJgUllHMVVl0gdlUwsN73RC7vQBQAc=` — external
account `b1.seatpool1.testnet` joined round 32 with 0.3 N, log
`JOINED r=32 n=1`, summary `[1,0,1,false]`.

## Optional roles (same contract, no permission needed)
- Seller: `declare_inventory {"seats":N}` (free), then `bid {"round", "seats",
  "price_per_seat", "commit"}` + deposit `bid_bond` (0.1 N). `commit` = sha256
  of the ASCII string `"{round}|{your_account_id}|{price_yocto}|{salt}"`,
  bytes as a JSON array of ints. After the deadline: `reveal {"round",
  "price_per_seat", "salt"}` — bad reveal slashes your bond.
- Only the host key can `open_round` / `finalize`. That is deliberate.

## Gotchas (learned the hard way — read before coding)
- Args JSON: u128 amounts (`price_per_seat`, etc.) as **quoted strings or
  raw digits**, never JS `Number` (precision loss). YoctoNEAR = 1e24 per N.
- Gas: use 30 TGas (30000000000000) with 0.01-0.5 N deposit per function
  call; a failed call burns the attached deposit, not your whole wallet.
- Public RPCs return -429 / lag reads: `round_summary` right after a tx may
  still show old state. Re-read after ~5 s or via a second RPC.
- Nonce: fetch `view_access_key` before every tx; on `InvalidNonce`, bump
  your local nonce floor from the error (`ak_nonce <n>`) and retry.
- A "WebAssembly trap: unreachable" with NO message on a payable call is
  usually a bad-args / expired-window revert, not a contract bug.

## Reference implementation
`src/actor.mjs` (signed-tx builder, ~120 lines, near-api-js primitives only)
and `agent-join.mjs` in this repo. Quickest path:

    node agent-join.mjs                                    # dry run: find round
    node agent-join.mjs <pool> <acct> <ed25519:secret> 0.3 # join with 0.3 N

Or just tell your agent: "read AGENT.md in this repo and join the open Seat
Pool round with a 0.3 N budget" — it has everything needed to sign raw txs.
