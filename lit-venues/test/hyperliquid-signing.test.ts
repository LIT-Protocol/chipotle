import { describe, expect, it } from 'vitest';
import {
  APPROVE_AGENT_FIELDS,
  USD_SEND_FIELDS,
  WITHDRAW_FIELDS,
  actionHash,
  phantomAgent,
  signL1Action,
  signUserSignedAction,
} from '../src/venues/hyperliquid-signing';
import { privateKeyToAddress, rawKeySigner } from '../src/eip712';
import { msgpackEncode } from '../src/msgpack';
import { bytesToHex } from '@noble/hashes/utils';

/**
 * Every vector below is lifted verbatim from the official Python SDK's
 * tests/signing_test.py (hyperliquid-dex/hyperliquid-python-sdk). If one of
 * these fails, our msgpack/EIP-712 bytes have drifted from what the venue
 * verifies — do NOT adjust the expectations; fix the encoding.
 */

const SDK_KEY = '0x0123456789012345678901234567890123456789012345678901234567890123';
const sign = rawKeySigner(SDK_KEY);

// float_to_int_for_hashing(1000) — the SDK's dummy action payload
const DUMMY_ACTION = { type: 'dummy', num: 100_000_000_000 };

function orderAction(asset: number, t: unknown, cloid?: string) {
  const wire: Record<string, unknown> = {
    a: asset,
    b: true,
    p: '100',
    s: '100',
    r: false,
    t,
  };
  if (cloid) wire.c = cloid;
  return { type: 'order', orders: [wire], grouping: 'na' };
}

describe('hyperliquid signing — official SDK vectors', () => {
  it('phantom agent connectionId matches production', () => {
    const action = {
      type: 'order',
      orders: [{ a: 4, b: true, p: '1670.1', s: '0.0147', r: false, t: { limit: { tif: 'Ioc' } } }],
      grouping: 'na',
    };
    const hash = actionHash(action, 1677777606040);
    expect(phantomAgent(hash, true).connectionId).toBe(
      '0x0fcbeda5ae3c4950a548021552a4fea2226858c4453571bf3f24ba017eac2908',
    );
  });

  it('signs the dummy L1 action (mainnet + testnet)', async () => {
    expect(await signL1Action(sign, DUMMY_ACTION, 0, { isMainnet: true })).toEqual({
      r: '0x53749d5b30552aeb2fca34b530185976545bb22d0b3ce6f62e31be961a59298',
      s: '0x755c40ba9bf05223521753995abb2f73ab3229be8ec921f350cb447e384d8ed8',
      v: 27,
    });
    expect(await signL1Action(sign, DUMMY_ACTION, 0, { isMainnet: false })).toEqual({
      r: '0x542af61ef1f429707e3c76c5293c80d01f74ef853e34b76efffcb57e574f9510',
      s: '0x17b8b32f086e8cdede991f1e2c529f5dd5297cbe8128500e00cbaf766204a613',
      v: 28,
    });
  });

  it('signs a GTC limit order action', async () => {
    const action = orderAction(1, { limit: { tif: 'Gtc' } });
    expect(await signL1Action(sign, action, 0, { isMainnet: true })).toEqual({
      r: '0xd65369825a9df5d80099e513cce430311d7d26ddf477f5b3a33d2806b100d78e',
      s: '0x2b54116ff64054968aa237c20ca9ff68000f977c93289157748a3162b6ea940e',
      v: 28,
    });
    expect(await signL1Action(sign, action, 0, { isMainnet: false })).toEqual({
      r: '0x82b2ba28e76b3d761093aaded1b1cdad4960b3af30212b343fb2e6cdfa4e3d54',
      s: '0x6b53878fc99d26047f4d7e8c90eb98955a109f44209163f52d8dc4278cbbd9f5',
      v: 27,
    });
  });

  it('signs an order with a cloid (the "c" key is appended last)', async () => {
    const action = orderAction(1, { limit: { tif: 'Gtc' } }, '0x00000000000000000000000000000001');
    expect(await signL1Action(sign, action, 0, { isMainnet: true })).toEqual({
      r: '0x41ae18e8239a56cacbc5dad94d45d0b747e5da11ad564077fcac71277a946e3',
      s: '0x3c61f667e747404fe7eea8f90ab0e76cc12ce60270438b2058324681a00116da',
      v: 27,
    });
    expect(await signL1Action(sign, action, 0, { isMainnet: false })).toEqual({
      r: '0xeba0664bed2676fc4e5a743bf89e5c7501aa6d870bdb9446e122c9466c5cd16d',
      s: '0x7f3e74825c9114bc59086f1eebea2928c190fdfbfde144827cb02b85bbe90988',
      v: 28,
    });
  });

  it('signs with a vault address (0x01 marker + 20 bytes appended to the hash input)', async () => {
    const vault = '0x1719884eb866cb12b2287399b15f7db5e7d775ea';
    expect(await signL1Action(sign, DUMMY_ACTION, 0, { isMainnet: true, vaultAddress: vault })).toEqual({
      r: '0x3c548db75e479f8012acf3000ca3a6b05606bc2ec0c29c50c515066a326239',
      s: '0x4d402be7396ce74fbba3795769cda45aec00dc3125a984f2a9f23177b190da2c',
      v: 28,
    });
    expect(await signL1Action(sign, DUMMY_ACTION, 0, { isMainnet: false, vaultAddress: vault })).toEqual({
      r: '0xe281d2fb5c6e25ca01601f878e4d69c965bb598b88fac58e475dd1f5e56c362b',
      s: '0x7ddad27e9a238d045c035bc606349d075d5c5cd00a6cd1da23ab5c39d4ef0f60',
      v: 27,
    });
  });

  it('signs a TP/SL trigger order', async () => {
    const action = orderAction(1, { trigger: { isMarket: true, triggerPx: '103', tpsl: 'sl' } });
    expect(await signL1Action(sign, action, 0, { isMainnet: true })).toEqual({
      r: '0x98343f2b5ae8e26bb2587daad3863bc70d8792b09af1841b6fdd530a2065a3f9',
      s: '0x6b5bb6bb0633b710aa22b721dd9dee6d083646a5f8e581a20b545be6c1feb405',
      v: 27,
    });
    expect(await signL1Action(sign, action, 0, { isMainnet: false })).toEqual({
      r: '0x971c554d917c44e0e1b6cc45d8f9404f32172a9d3b3566262347d0302896a2e4',
      s: '0x206257b104788f80450f8e786c329daa589aa0b32ba96948201ae556d5637eac',
      v: 28,
    });
  });

  it('signs scheduleCancel (dead-man switch), with and without time', async () => {
    expect(await signL1Action(sign, { type: 'scheduleCancel' }, 0, { isMainnet: true })).toEqual({
      r: '0x6cdfb286702f5917e76cd9b3b8bf678fcc49aec194c02a73e6d4f16891195df9',
      s: '0x6557ac307fa05d25b8d61f21fb8a938e703b3d9bf575f6717ba21ec61261b2a0',
      v: 27,
    });
    expect(await signL1Action(sign, { type: 'scheduleCancel', time: 123456789 }, 0, { isMainnet: true })).toEqual({
      r: '0x609cb20c737945d070716dcc696ba030e9976fcf5edad87afa7d877493109d55',
      s: '0x16c685d63b5c7a04512d73f183b3d7a00da5406ff1f8aad33f8ae2163bab758b',
      v: 28,
    });
  });

  it('signs createSubAccount and subAccountTransfer actions', async () => {
    expect(await signL1Action(sign, { type: 'createSubAccount', name: 'example' }, 0, { isMainnet: true })).toEqual({
      r: '0x51096fe3239421d16b671e192f574ae24ae14329099b6db28e479b86cdd6caa7',
      s: '0xb71f7d293af92d3772572afb8b102d167a7cef7473388286bc01f52a5c5b423',
      v: 27,
    });
    const transfer = {
      type: 'subAccountTransfer',
      subAccountUser: '0x1d9470d4b963f552e6f671a81619d395877bf409',
      isDeposit: true,
      usd: 10,
    };
    expect(await signL1Action(sign, transfer, 0, { isMainnet: true })).toEqual({
      r: '0x43592d7c6c7d816ece2e206f174be61249d651944932b13343f4d13f306ae602',
      s: '0x71a926cb5c9a7c01c3359ec4c4c34c16ff8107d610994d4de0e6430e5cc0f4c9',
      v: 28,
    });
  });

  it('signs user-signed actions: UsdSend and Withdraw (HyperliquidSignTransaction domain)', async () => {
    const message = {
      hyperliquidChain: 'Testnet',
      destination: '0x5e9ee1089755c3435139848e47e6635505d5a13a',
      amount: '1',
      time: 1687816341423,
    };
    expect(await signUserSignedAction(sign, message, USD_SEND_FIELDS, 'HyperliquidTransaction:UsdSend')).toEqual({
      r: '0x637b37dd731507cdd24f46532ca8ba6eec616952c56218baeff04144e4a77073',
      s: '0x11a6a24900e6e314136d2592e2f8d502cd89b7c15b198e1bee043c9589f9fad7',
      v: 27,
    });
    expect(await signUserSignedAction(sign, message, WITHDRAW_FIELDS, 'HyperliquidTransaction:Withdraw')).toEqual({
      r: '0x8363524c799e90ce9bc41022f7c39b4e9bdba786e5f9c72b20e43e1462c37cf9',
      s: '0x58b1411a775938b83e29182e8ef74975f9054c8e97ebf5ec2dc8d51bfc893881',
      v: 28,
    });
  });

  it('approveAgent uses the documented field set (no published SDK vector; shape-checked)', async () => {
    const sig = await signUserSignedAction(
      sign,
      { hyperliquidChain: 'Testnet', agentAddress: '0x' + '11'.repeat(20), agentName: 'lit-policy', nonce: 1 },
      APPROVE_AGENT_FIELDS,
      'HyperliquidTransaction:ApproveAgent',
    );
    expect(sig.r).toMatch(/^0x[0-9a-f]+$/);
    expect([27, 28]).toContain(sig.v);
  });
});

describe('msgpack encoder', () => {
  it('encodes the canonical order action like Python msgpack.packb', () => {
    const bytes = msgpackEncode({ type: 'order', orders: [], grouping: 'na' });
    expect(bytesToHex(bytes)).toBe(
      // fixmap(3) ‖ "type" ‖ "order" ‖ "orders" ‖ fixarray(0) ‖ "grouping" ‖ "na"
      '83a474797065a56f72646572a66f726465727390a867726f7570696e67a26e61',
    );
  });

  it('uses minimal int widths exactly like Python msgpack (fixint / uint32 / uint64)', () => {
    expect(bytesToHex(msgpackEncode([4, 300, 123456789, 100_000_000_000]))).toBe(
      '9404cd012cce075bcd15cf000000174876e800',
    );
  });

  it('refuses non-integer numbers (float drift would change the action hash)', () => {
    expect(() => msgpackEncode({ p: 1670.1 })).toThrow(/decimal strings/);
  });

  it('skips undefined values like JSON.stringify does', () => {
    expect(msgpackEncode({ a: 1, b: undefined })).toEqual(msgpackEncode({ a: 1 }));
  });
});

describe('eip712 helpers', () => {
  it('derives the Ethereum address of the SDK fixture key', () => {
    expect(privateKeyToAddress(SDK_KEY)).toMatch(/^0x[0-9a-f]{40}$/);
  });
});
