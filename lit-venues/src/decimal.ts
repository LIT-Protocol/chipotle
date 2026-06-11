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

/** Largest multiple of `increment` that is <= `value`. For sizing orders to venue lot/tick rules. */
export function roundDownToIncrement(value: string, increment: string): string {
  const dp = Math.max(decimalsOf(value), decimalsOf(increment));
  const v = toScaled(value, dp);
  const inc = toScaled(increment, dp);
  if (inc <= 0n) throw new Error('increment must be > 0');
  return fromScaled((v / inc) * inc, dp);
}
