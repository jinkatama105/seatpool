// Seat Pool - live testnet demo. Put your testnet key at keys/root3.json (gitignored). Run: node demo.mjs
import fs from 'fs';
import crypto from 'crypto';
import { Actor, A, view } from './src/actor.mjs';
const r3 = JSON.parse(fs.readFileSync('keys/root3.json', 'utf8'));
const ra = Actor.fromSecret(r3.secret_key, r3.account_id);
const POOL = 'pool2.' + r3.account_id;
const BOB = 'bob.' + r3.account_id, SAM = 'sam.' + r3.account_id;
const bob = Actor.fromSecret(r3.secret_key, BOB), sam = Actor.fromSecret(r3.secret_key, SAM);
const fc = (m, o, g = 30_000_000_000_000n, d = 0n) => A.functionCall(m, o, g, d);
const N = 10n ** 24n;
const ri = (x) => JSON.rawJSON(x.toString());
const near = (b) => (Number(b) / 1e24).toFixed(3) + ' N';
const bal = async (a) => view.account(a).then(j => BigInt(j.amount));

console.log('=== SEAT POOL — NEAR testnet live demo ===\n');
const [b0, s0, p0] = [await bal(BOB), await bal(SAM), await bal(POOL)];
console.log(`before: bob ${near(b0)} | sam(seller) ${near(s0)} | pool ${near(p0)}\n`);

const blk = await view.block('final');
const deadline = Number(BigInt(blk.header.timestamp_nanosec)) + 90e9;
const bond = N / 10n, price = 4n * N / 10n;

console.log('1) host opens an auction round (deadline 90s, bid bond 0.1 N)');
let r = await ra.send([fc('open_round', { deadline_ns: deadline, min_pool: 0, bid_bond: ri(bond) })], { receiver: POOL });
const round = Number(Buffer.from(r.status.SuccessValue, 'base64').toString());

console.log('2) seller declares 2 seats for sale');
await sam.send([fc('declare_inventory', { seats: 2 })], { receiver: POOL });

console.log('3) buyer bob joins the pool with 1 N');
await bob.send([fc('join', { round }, 30_000_000_000_000n, N)], { receiver: POOL });

console.log('4) seller bids blindly: 2 seats @ 0.4 N (hashed commit first)');
const salt = crypto.randomBytes(8).toString('hex');
const commit = crypto.createHash('sha256').update(`${round}|${SAM}|${price}|${salt}`).digest();
await sam.send([fc('bid', { round, seats: 2, price_per_seat: ri(price), commit: [...commit] }, 30_000_000_000_000n, bond)], { receiver: POOL });

console.log('5) wait for deadline...');
await new Promise(res => setTimeout(res, 100000));

console.log('6) seller reveals the price');
r = await sam.send([fc('reveal', { round, price_per_seat: ri(price), salt })], { receiver: POOL });
console.log('   ' + r.logs[0]);

console.log('7) finalize -> median cap applied, seat passes minted, money moves');
r = await ra.send([fc('finalize', { round })], { receiver: POOL });
for (const l of r.logs) console.log('   ' + l);

const [b1, s1, p1] = [await bal(BOB), await bal(SAM), await bal(POOL)];
console.log(`\nafter:  bob ${near(b1)} (-${near(b0 - b1)}) | sam ${near(s1)} (+${near(s1 - s0)}) | pool ${near(p1)}`);
console.log('bob owns pass:', await view.call(POOL, 'get_pass', '{"seat_id":1}'));
