import { VenueError } from './errors';
import { BinanceClient } from './venues/binance';
import { CoinbaseClient } from './venues/coinbase';
import { HyperliquidClient } from './venues/hyperliquid';
import type { VenueClient, VenueConfig } from './types';

export const VERSION = '0.2.0';

export function createVenue(cfg: VenueConfig): VenueClient {
  switch (cfg.venueId) {
    case 'binance':
    case 'binanceus':
      return new BinanceClient(cfg);
    case 'coinbase':
      return new CoinbaseClient(cfg);
    case 'hyperliquid':
      return new HyperliquidClient(cfg);
    default:
      throw new VenueError(String(cfg.venueId), 'invalid_request', `unknown venueId "${String(cfg.venueId)}"`);
  }
}

export { BinanceClient } from './venues/binance';
export { CoinbaseClient } from './venues/coinbase';
export { HyperliquidClient } from './venues/hyperliquid';
export { litActionProxiedFetch, resolveFetch } from './transports';
export { VenueError, type VenueErrorCode } from './errors';
export {
  addDec,
  subDec,
  roundDownToIncrement,
  decimalsOf,
  wireDecimal,
  sigFigsOf,
  floorToSigFigs,
  applyBps,
} from './decimal';
export {
  hmacSha256Hex,
  sha256Hex,
  ed25519SignBase64,
  es256Jwt,
  randomHex,
  b64encode,
  b64decode,
  b64urlEncode,
  parseEd25519PrivateKey,
  parseEcP256PrivateKey,
} from './signing';
export { msgpackEncode } from './msgpack';
export {
  typedDataDigest,
  hashStruct,
  privateKeyToAddress,
  rawKeySigner,
  ZERO_ADDRESS,
  type RsvSignature,
  type SignFn,
  type Eip712Field,
  type Eip712Domain,
} from './eip712';
export {
  actionHash,
  phantomAgent,
  signL1Action,
  signUserSignedAction,
  APPROVE_AGENT_FIELDS,
  USD_SEND_FIELDS,
  WITHDRAW_FIELDS,
  HYPERLIQUID_SIGNATURE_CHAIN_ID,
} from './venues/hyperliquid-signing';
export type * from './types';
