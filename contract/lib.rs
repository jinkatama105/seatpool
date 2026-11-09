//! Seat Pool Coordinator — NEAR hackathon 2026 (tracks: swarms + agents)
//! Spec: RULES.md §2 lifecycle, §3 anti-cheat R1-R5.
//! Simulated goods, real settlement: every NEAR here moves on-testnet for real.
//! near-sdk 5.30 idioms: store collections, NearToken, u32 vector indices.

near_sdk::setup_alloc!();

use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::store::{IterableMap, LookupMap, UnorderedMap, Vector};
use near_sdk::{env, near, near_bindgen, AccountId, NearToken, PanicOnDefault, Promise};

type Balance = u128;

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Bid {
    pub seller: AccountId,
    pub seats: u64,
    pub price_per_seat: Balance, // yoctoNEAR
    pub commit: Vec<u8>,         // sha256(round|seller|price|salt)
    pub revealed: bool,
    pub bond: Balance,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Round {
    pub id: u64,
    pub deadline_ns: u64,
    pub seats_total: u64,
    pub closed: bool,
    pub min_pool: u64,
    pub bid_bond: Balance,
    pub buyers: Vec<(AccountId, Balance)>, // join order preserved
    pub bids: Vec<Bid>,
    pub escrow: Balance,
    pub seats_sold: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SeatPass {
    pub seat_id: u64,
    pub owner: AccountId,
    pub sku: String,
    pub expiry_ns: u64,
    pub round_id: u64,
    pub gross_price: Balance, // simulated retail reference
    pub paid_share: Balance,  // what the swarm actually paid per seat
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Coordinator {
    owner: AccountId,
    treasury: AccountId,
    rounds: Vector<Round>,
    inventory: IterableMap<AccountId, u64>, // R3: declared stock per seller
    passes: UnorderedMap<u64, SeatPass>,
    pass_counter: u64,
    seller_rounds: LookupMap<String, bool>, // R5: one bid per seller per round
}

fn yocto(v: Balance) -> NearToken {
    NearToken::from_yoctonear(v)
}

#[near_bindgen]
impl Coordinator {
    #[init]
    pub fn new(owner: AccountId, treasury: AccountId) -> Self {
        assert!(!env::state_exists(), "E20: already initiated");
        Self::build(owner, treasury)
    }

    /// Reset state over a legacy contract (stale borsh layout from SDK4-era deploys).
    #[init(ignore_state)]
    pub fn fresh_init(owner: AccountId, treasury: AccountId) -> Self {
        Self::build(owner, treasury)
    }

    fn build(owner: AccountId, treasury: AccountId) -> Self {
        Self {
            owner,
            treasury,
            rounds: Vector::new(b"r".to_vec()),
            inventory: IterableMap::new(b"i".to_vec()),
            passes: UnorderedMap::new(b"p".to_vec()),
            pass_counter: 0,
            seller_rounds: LookupMap::new(b"s".to_vec()),
        }
    }

    /// Sellers pre-register simulated stock. R3 anchor.
    pub fn declare_inventory(&mut self, seats: u64) {
        let seller = env::predecessor_account_id();
        self.inventory.insert(seller, seats);
    }

    /// §2.1 OPEN
    pub fn open_round(&mut self, deadline_ns: u64, min_pool: u64, bid_bond: Balance) -> u64 {
        assert_eq!(env::predecessor_account_id(), self.owner, "E30: owner only");
        let id = self.rounds.len() as u64;
        self.rounds.push(Round {
            id,
            deadline_ns,
            seats_total: 0,
            closed: false,
            min_pool,
            bid_bond,
            buyers: vec![],
            bids: vec![],
            escrow: 0,
            seats_sold: 0,
        });
        env::log_str(&format!("ROUND_OPEN {id} deadline_ns={deadline_ns}"));
        id
    }

    /// §2.2 POOL — attached deposit = buyer budget (escrowed from here).
    #[payable]
    pub fn join(&mut self, round: u64) {
        let buyer = env::predecessor_account_id();
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(deposit > 0, "E31: must attach budget");
        let r = self.rounds.get_mut(round as u32).expect("E32: no round");
        assert!(!r.closed, "E33: closed");
        assert!(env::block_timestamp() < r.deadline_ns, "E34: pool window over");
        assert!(!r.buyers.iter().any(|(a, _)| a == &buyer), "E35: dup join");
        r.escrow += deposit;
        r.seats_total += 1;
        r.buyers.push((buyer, deposit));
        env::log_str(&format!("JOINED r={round} n={}", r.buyers.len()));
    }

    /// §2.4 SEALED BID — bond attached.
    #[payable]
    pub fn bid(&mut self, round: u64, seats: u64, price_per_seat: Balance, commit: Vec<u8>) {
        let seller = env::predecessor_account_id();
        let deposit = env::attached_deposit().as_yoctonear();
        let r = self.rounds.get_mut(round as u32).expect("E32: no round");
        assert!(!r.closed, "E33: closed");
        assert!(env::block_timestamp() < r.deadline_ns, "E34: window over");
        assert!(deposit >= r.bid_bond, "E36: bond too small");
        let sr_key = format!("{round}:{seller}");
        assert!(!self.seller_rounds.contains_key(&sr_key), "E37: R5 one-bid/seller/round");
        let stock = self.inventory.get(&seller).copied().unwrap_or(0);
        assert!(seats <= stock, "E38: R3 oversell");
        r.bids.push(Bid { seller: seller.clone(), seats, price_per_seat, commit, revealed: false, bond: deposit });
        self.seller_rounds.insert(sr_key, true);
        env::log_str(&format!("BID r={round} s={seller} seats={seats}"));
    }

    /// §2.5 REVEAL — verify hash; slash on mismatch (R1).
    pub fn reveal(&mut self, round: u64, price_per_seat: Balance, salt: String) {
        let seller = env::predecessor_account_id();
        let r = self.rounds.get_mut(round as u32).expect("E32: no round");
        assert!(env::block_timestamp() >= r.deadline_ns, "E39: too early");
        let mut slashed: Balance = 0;
        let mut my_bids = 0usize;
        for b in r.bids.iter_mut().filter(|b| b.seller == seller && !b.revealed) {
            my_bids += 1;
            let expect = Self::commit_hash(round, &seller, price_per_seat, &salt);
            if b.commit == expect {
                b.revealed = true;
                env::log_str(&format!("REVEALED r={round} s={seller} price={price_per_seat}"));
            } else {
                slashed += b.bond;
                b.bond = 0;
                env::log_str(&format!("SLASH r={round} s={seller} bad-reveal"));
            }
        }
        assert!(my_bids > 0, "E41: no unrevealed bid for seller");
        drop(r);
        if slashed > 0 {
            Promise::new(self.treasury.clone()).transfer(yocto(slashed));
        }
    }

    /// §2.6–2.8 CLEAR + SETTLE + CLOSE. Deterministic.
    pub fn finalize(&mut self, round: u64) {
        {
            let r = self.rounds.get_mut(round as u32).expect("E32: no round");
            assert!(!r.closed, "E33: closed");
            assert!(env::block_timestamp() >= r.deadline_ns, "E39: too early");
        }
        // snapshot to locals so we can mutate other maps while paying
        let (buyers, bids, min_pool, seats_total) = {
            let r = self.rounds.get(round as u32).expect("E32");
            (r.buyers.clone(), r.bids.clone(), r.min_pool, r.seats_total)
        };

        // under min pool → full refund, bonds returned
        if (buyers.len() as u64) < min_pool {
            for (buyer, dep) in buyers.iter() {
                Promise::new(buyer.clone()).transfer(yocto(*dep));
            }
            for b in bids.iter() {
                Promise::new(b.seller.clone()).transfer(yocto(b.bond));
            }
            let r = self.rounds.get_mut(round as u32).expect("E32");
            r.closed = true;
            r.escrow = 0;
            env::log_str(&format!("REFUND_ONLY r={round}"));
            return;
        }

        // R4: median budget computed only now — never visible before bids locked
        let mut budgets: Vec<Balance> = buyers.iter().map(|(_, b)| *b).collect();
        budgets.sort();
        let cap = budgets[budgets.len() / 2];

        let mut revealed: Vec<Bid> = bids.iter().filter(|b| b.revealed).cloned().collect();
        revealed.sort_by(|a, b| a.price_per_seat.cmp(&b.price_per_seat)
            .then(a.seller.as_str().cmp(b.seller.as_str())));

        let seats_needed = seats_total;
        let mut fills: Vec<(Bid, u64)> = vec![];
        let mut remaining = seats_needed;
        for bid in revealed.into_iter() {
            if remaining == 0 {
                break;
            }
            if bid.price_per_seat > cap {
                continue; // median buyer can't afford → skip
            }
            let take = bid.seats.min(remaining);
            fills.push((bid, take));
            remaining -= take;
        }

        let mut bi = 0usize;
        for (bid, take) in fills.iter() {
            let fill_cost = bid.price_per_seat * (*take as u128);
            Promise::new(bid.seller.clone()).transfer(yocto(fill_cost + bid.bond));
            self.inventory_update(&bid.seller, *take);
            for _ in 0..*take {
                let (buyer, budget) = buyers[bi].clone();
                bi += 1;
                let share = fill_cost / *take as u128;
                assert!(budget >= share, "E40: budget below cleared price");
                Promise::new(buyer.clone()).transfer(yocto(budget - share));
                self.pass_counter += 1;
                let pass = SeatPass {
                    seat_id: self.pass_counter,
                    owner: buyer.clone(),
                    sku: "swarm-seat-lic".to_string(),
                    expiry_ns: env::block_timestamp() + 86_400_000_000_000,
                    round_id: round,
                    gross_price: 60_000_000_000_000_000_000_000,
                    paid_share: share,
                };
                self.passes.insert(self.pass_counter, pass);
                env::log_str(&format!("SEATPASS {} {} paid {}", self.pass_counter, buyer, share));
            }
        }
        for (buyer, budget) in buyers.iter().skip(bi) {
            Promise::new(buyer.clone()).transfer(yocto(*budget));
        }
        for b in bids.iter().filter(|b| b.revealed) {
            if !fills.iter().any(|(f, _)| f.seller == b.seller) {
                Promise::new(b.seller.clone()).transfer(yocto(b.bond));
            }
        }
        for b in bids.iter().filter(|b| !b.revealed) {
            Promise::new(self.treasury.clone()).transfer(yocto(b.bond)); // R1 slash
        }
        let r = self.rounds.get_mut(round as u32).expect("E32");
        r.seats_sold = seats_needed - remaining;
        r.closed = true;
        r.escrow = 0;
        env::log_str(&format!("FINAL r={round} cap={cap} sold={} remaining={remaining}", r.seats_sold));
    }

    // ---- views ----
    pub fn round_summary(&self, round: u64) -> (u64, u64, u64, bool) {
        let r = self.rounds.get(round as u32).expect("E32");
        (r.buyers.len() as u64, r.bids.iter().filter(|b| b.revealed).count() as u64, r.seats_total, r.closed)
    }
    pub fn peek_median(&self, round: u64) -> Option<u128> {
        let r = self.rounds.get(round as u32)?;
        if env::block_timestamp() < r.deadline_ns {
            return None;
        }
        let mut b: Vec<Balance> = r.buyers.iter().map(|(_, x)| *x).collect();
        if b.is_empty() {
            return None;
        }
        b.sort();
        Some(b[b.len() / 2])
    }
    pub fn get_pass(&self, seat_id: u64) -> Option<SeatPass> {
        self.passes.get(&seat_id).cloned()
    }
    pub fn passes_of(&self, buyer: AccountId) -> Vec<u64> {
        self.passes.iter().filter(|(_, p)| p.owner == buyer).map(|(k, _)| *k).collect()
    }

    // ---- internal ----
    fn commit_hash(round: u64, seller: &AccountId, price: Balance, salt: &str) -> Vec<u8> {
        let payload = format!("{round}|{seller}|{price}|{salt}");
        env::sha256(payload.as_bytes())
    }
    fn inventory_update(&mut self, seller: &AccountId, taken: u64) {
        let s = self.inventory.get(seller).copied().unwrap_or(0);
        self.inventory.insert(seller.clone(), s.saturating_sub(taken));
    }
}
