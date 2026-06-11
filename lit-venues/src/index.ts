import { VenueError } from './errors';
import { BinanceClient } from './venues/binance';
import { CoinbaseClient } from './venues/coinbase';
import type { VenueClient, VenueConfig } from './types';

export const VERSION = '0.1.0';

export function createVenue(cfg: VenueConfig): VenueClient {
  switch (cfg.venueId) {
    case 'binance':
    case 'binanceus':
      return new BinanceClient(cfg);
    case 'coinbase':
      return new CoinbaseClient(cfg);
    default:
      throw new VenueError(String(cfg.venueId), 'invalid_request', `unknown venueId "${String(cfg.venueId)}"`);
  }
}

export { BinanceClient } from './venues/binance';
export { CoinbaseClient } from './venues/coinbase';
export { litActionProxiedFetch, resolveFetch } from './transports';
export { VenueError, type VenueErrorCode } from './errors';
export { addDec, roundDownToIncrement, decimalsOf } from './decimal';
export {
  hmacSha256Hex,
  ed25519SignBase64,
  es256Jwt,
  randomHex,
  b64encode,
  b64decode,
  b64urlEncode,
  parseEd25519PrivateKey,
  parseEcP256PrivateKey,
} from './signing';
export type * from './types';
