/**
 * Exact decimal-string arithmetic over scaled BigInts. Venue amounts/prices
 * are decimal strings end to end; nothing here round-trips through floats.
 */

export function decimalsOf(s: string): number {
  const i = s.indexOf('.');
  return i < 0 ? 0 : s.length - i - 1;
}

function toScaled(s: string, dp: number): bigint {
  const neg = s.startsWith('-');
  const t = neg ? s.slice(1) : s;
  const dot = t.indexOf('.');
  const int = dot < 0 ? t : t.slice(0, dot);
  const frac = dot < 0 ? '' : t.slice(dot + 1);
  if (!/^\d*$/.test(int) || !/^\d*$/.test(frac) || (int === '' && frac === '')) {
    throw new Error(`invalid decimal: "${s}"`);
  }
  if (frac.length > dp) throw new Error(`decimal "${s}" exceeds working precision ${dp}`);
  const scaled = BigInt((int || '0') + frac.padEnd(dp, '0'));
  return neg ? -scaled : scaled;
}

function fromScaled(v: bigint, dp: number): string {
  const neg = v < 0n;
  const a = (neg ? -v : v).toString().padStart(dp + 1, '0');
  const int = a.slice(0, a.length - dp) || '0';
  const frac = dp ? a.slice(a.length - dp).replace(/0+$/, '') : '';
  return `${neg ? '-' : ''}${int}${frac ? '.' + frac : ''}`;
}

export function addDec(a: string, b: string): string {
  const dp = Math.max(decimalsOf(a), decimalsOf(b));
  return fromScaled(toScaled(a, dp) + toScaled(b, dp), dp);
}

export function subDec(a: string, b: string): string {
  const dp = Math.max(decimalsOf(a), decimalsOf(b));
  return fromScaled(toScaled(a, dp) - toScaled(b, dp), dp);
}

/**
 * Normalize a decimal string to Hyperliquid wire form — what the Python SDK's
 * `float_to_wire` produces: no trailing fractional zeros, no trailing dot, no
 * leading zeros, "-0" → "0". The signed action hash commits to these exact
 * bytes, so order px/sz MUST pass through here.
 */
export function wireDecimal(s: string): string {
  let t = s.trim();
  const neg = t.startsWith('-');
  if (neg) t = t.slice(1);
  if (t.startsWith('.')) t = `0${t}`;
  if (!/^\d+(\.\d+)?$/.test(t)) throw new Error(`invalid decimal: "${s}"`);
  const dot = t.indexOf('.');
  let int = dot < 0 ? t : t.slice(0, dot);
  let frac = dot < 0 ? '' : t.slice(dot + 1);
  int = int.replace(/^0+(?=\d)/, '');
  frac = frac.replace(/0+$/, '');
  const out = frac ? `${int}.${frac}` : int;
  if (out === '0') return '0';
  return neg ? `-${out}` : out;
}

/** Significant figures of a (wire-normalized) decimal. Integers are exempt from Hyperliquid's 5-sig-fig price rule, so callers only consult this for fractional prices. */
export function sigFigsOf(s: string): number {
  const digits = wireDecimal(s).replace('-', '').replace('.', '').replace(/^0+/, '');
  return digits.length;
}

/** Floor `value` to at most `figs` significant figures (zeroing lower digits). Used to make aggressive IOC prices venue-valid. */
export function floorToSigFigs(value: string, figs: number): string {
  const t = wireDecimal(value);
  if (t.startsWith('-')) throw new Error('floorToSigFigs: negative values unsupported');
  let seen = 0;
  let out = '';
  for (const ch of t) {
    if (ch === '.') {
      out += ch;
      continue;
    }
    if (seen >= figs || (seen === 0 && ch === '0')) {
      out += seen >= figs ? '0' : ch;
      continue;
    }
    out += ch;
    seen++;
  }
  return wireDecimal(out);
}

/** value × (10000 + bps) / 10000, floored at `maxDecimals` decimals. Exact bigint math — used for slippage-adjusted IOC prices. */
export function applyBps(value: string, bps: number, maxDecimals: number): string {
  const dp = decimalsOf(value);
  const scaled = toScaled(value, dp) * BigInt(10000 + bps);
  // now at dp+4 implied decimals; floor to maxDecimals
  const drop = dp + 4 - maxDecimals;
  const floored = drop > 0 ? scaled / 10n ** BigInt(drop) : scaled * 10n ** BigInt(-drop);
  return wireDecimal(fromScaled(floored, Math.max(maxDecimals, 0)));
}

/** Largest multiple of `increment` that is <= `value`. For sizing orders to venue lot/tick rules. */
export function roundDownToIncrement(value: string, increment: string): string {
  const dp = Math.max(decimalsOf(value), decimalsOf(increment));
  const v = toScaled(value, dp);
  const inc = toScaled(increment, dp);
  if (inc <= 0n) throw new Error('increment must be > 0');
  return fromScaled((v / inc) * inc, dp);
}
