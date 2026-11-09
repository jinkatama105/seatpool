// Minimal NEAR testnet actor for near-api-js v7 (bare-bones lib).
// Reads via raw JSON-RPC (v7 provider param quirks), writes via hand-built signed tx.
import {
  KeyPairEd25519, PublicKey, Transaction, SignedTransaction, Signature,
  actions, base64Encode, encodeTransaction,
} from 'near-api-js';
import nacl from 'tweetnacl';
import bs58 from 'bs58';
import { createHash } from 'crypto';

const RPC = 'https://rpc.testnet.near.org';
async function rpc(method, params) {
  for (let attempt = 0; ; attempt++) {
    const r = await fetch(RPC, {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: '1', method, params }),
    });
    const j = await r.json();
    if (j.error && (j.error.code === -429 || /DEPRECATED|429|RATE/i.test(JSON.stringify(j.error)))) {
      if (attempt < 12) { await new Promise(res => setTimeout(res, 5000 + attempt * 15000)); continue; }
    }
    if (j.error) throw new Error(`${method}: ${JSON.stringify(j.error).slice(0, 300)}`);
    return j.result;
  }
}
export const view = {
  block: (finality = 'final') => rpc('block', { finality }),
  account: (id) => rpc('query', { request_type: 'view_account', finality: 'final', account_id: id }),
  accessKey: (id, pk) => rpc('query', { request_type: 'view_access_key', finality: 'final', account_id: id, public_key: pk }),
  call: (id, fn, args = '{}') => rpc('query', { request_type: 'call_function', finality: 'final', account_id: id, method_name: fn, args_base64: base64Encode(Buffer.from(args)) })
    .then(r => {
      if (!r || !r.result) throw new Error('view-call failed: ' + JSON.stringify(r?.error ?? r).slice(0, 300));
      return Buffer.from(r.result).toString();
    }),
  txStatus: (id, hash) => rpc('tx', { tx_hash: hash, sender_account_id: id }),
};

export class Actor {
  constructor(accountId, keypair) { this.accountId = accountId; this.kp = keypair; }
  static fromSecret(secret, accountId) {
    // secret = "ed25519:<base58 of 64-byte key (seed32+pub32)>" — v7 dropped fromString
    const raw = bs58.decode(secret.replace(/^ed25519:/, ''));
    const keypair = nacl.sign.keyPair.fromSecretKey(raw);
    const kp = new KeyPairEd25519(bs58.encode(Buffer.from(keypair.secretKey)));
    return new Actor(accountId, kp);
  }
  static fromSeed(seedBytes, accountId) {
    const keypair = nacl.sign.keyPair.fromSeed(seedBytes);
    const kp = new KeyPairEd25519(bs58.encode(Buffer.from(keypair.secretKey)));
    return new Actor(accountId, kp);
  }
  static random(accountId) { return new Actor(accountId, KeyPairEd25519.fromRandom()); }
  pk() { return this.kp.getPublicKey(); }

  async send(actionList, opts = {}) {
    const receiver = opts.receiver || this.accountId;
    const pkStr = this.pk().toString();
    const ak = await view.accessKey(opts.nonceAccount || this.accountId, pkStr);
    const blk = await view.block('optimistic');
    const blockHash = bs58.decode(blk.header.hash);
    const nonce = BigInt(ak.nonce) + 1n;
    const tx = new Transaction({
      signerId: opts.nonceAccount || this.accountId, publicKey: this.pk(), receiverId: receiver,
      nonce, blockHash, actions: actionList,
    });
    // NEAR convention: signature over sha256(encodeTransaction(tx)) — UNSIGNED tx bytes.
    const txBytes = Buffer.from(encodeTransaction(tx));
    const hash = createHash('sha256').update(txBytes).digest();
    const signed = await this.kp.sign(hash); // returns {signature, publicKey}
    const stx = new SignedTransaction({
      transaction: tx,
      signature: new Signature({ keyType: 0, data: signed.signature }),
    });
    const wire = Buffer.from(stx.encode());
    const b64 = wire.toString('base64');
    const res = await rpc('broadcast_tx_commit', [b64]);
    const out = { hash: base64Encode(hash), status: res.status, logs: (res.receipts_outcome || []).flatMap(r => r.outcome.logs) };
    if (res.status && res.status.Failure) {
      throw new Error('TX FAILED: ' + JSON.stringify(res.status).slice(0, 600) + ' || logs: ' + out.logs.join(' | ').slice(0, 400));
    }
    return out;
  }
}

export const A = actions;
export { bs58, base64Encode, PublicKey };
