// Round #11 — "host & agents" narrated demo: root3 = host+seller, bob = joining agent-buyer
import fs from 'fs';
import crypto from 'crypto';
import { Actor, A, view } from './src/actor.mjs';
const r3 = JSON.parse(fs.readFileSync('keys/root3.json', 'utf8'));
const ra = Actor.fromSecret(r3.secret_key, r3.account_id);
const POOL = 'pool2.' + r3.account_id;
const BOB = 'bob.' + r3.account_id;
const bob = Actor.fromSecret(r3.secret_key, BOB);
const fc = (m, o, g = 30_000_000_000_000n, d = 0n) => A.functionCall(m, o, g, d);
const N = 10n ** 24n;
const ri = (x) => JSON.rawJSON(x.toString());
const near = (b) => (Number(b) / 1e24).toFixed(3) + ' N';
const bal = async (a) => view.account(a).then(j => BigInt(j.amount));

const [b0, h0, p0] = [await bal(BOB), await bal(r3.account_id), await bal(POOL)];
console.log(`before: bob ${near(b0)} | host ${near(h0)} | pool ${near(p0)}`);

const blk = await view.block('final');
const deadline = Number(BigInt(blk.header.timestamp_nanosec)) + 60e9;
const bond = N / 10n, price = 3n * N / 10n; // 0.1N bond, 0.3 N/seat

console.log('1) HOST opens round (owner-signed open_round)');
let r = await ra.send([fc('open_round', { deadline_ns: deadline, min_pool: 0, bid_bond: ri(bond) })], { receiver: POOL });
const round = Number(Buffer.from(r.status.SuccessValue, 'base64').toString());
console.log('   ROUND_OPEN ' + round);

console.log('2) host-as-seller declares 1 seat');
await ra.send([fc('declare_inventory', { seats: 1 })], { receiver: POOL });

console.log('3) AGENT bob joins pool with 0.55 N (payable join)');
await bob.send([fc('join', { round }, 30_000_000_000_000n, 55n * N / 100n)], { receiver: POOL });

console.log('4) seller blind-bids 1 seat @ 0.3 N (hashed commit)');
const salt = crypto.randomBytes(8).toString('hex');
const commit = crypto.createHash('sha256').update(`${round}|${r3.account_id}|${price}|${salt}`).digest();
await ra.send([fc('bid', { round, seats: 1, price_per_seat: ri(price), commit: [...commit] }, 30_000_000_000_000n, bond)], { receiver: POOL });

console.log('5) waiting out the 60s pool window...');
await new Promise(res => setTimeout(res, 70000));

console.log('6) seller reveals');
r = await ra.send([fc('reveal', { round, price_per_seat: ri(price), salt })], { receiver: POOL });
console.log('   ' + r.logs[0]);

console.log('7) finalize -> cap, pass mint, payout');
r = await ra.send([fc('finalize', { round })], { receiver: POOL });
for (const l of r.logs) console.log('   ' + l);

const [b1, h1, p1] = [await bal(BOB), await bal(r3.account_id), await bal(POOL)];
console.log(`after: bob ${near(b1)} | host ${near(h1)} | pool ${near(p1)}`);
console.log('passes_of(bob):', await view.call(POOL, 'passes_of', `{"buyer":"${BOB}"}`));
console.log('summary:', await view.call(POOL, 'round_summary', `{"round":${round}}`));
