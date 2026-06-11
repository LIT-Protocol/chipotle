import { describe, expect, it } from 'vitest';
import { addDec, decimalsOf, roundDownToIncrement } from '../src/decimal';

describe('decimal-string arithmetic', () => {
  it('adds without float drift', () => {
    expect(addDec('0.1', '0.2')).toBe('0.3');
    expect(addDec('1', '0.000001')).toBe('1.000001');
    expect(addDec('0.00000000', '0')).toBe('0');
    expect(addDec('-0.5', '0.75')).toBe('0.25');
  });

  it('rounds down to venue increments', () => {
    expect(roundDownToIncrement('1.2345', '0.01')).toBe('1.23');
    expect(roundDownToIncrement('5', '1')).toBe('5');
    expect(roundDownToIncrement('0.123456789', '0.0001')).toBe('0.1234');
    expect(roundDownToIncrement('0.00009', '0.0001')).toBe('0');
  });

  it('rejects invalid input', () => {
    expect(() => addDec('1.2.3', '0')).toThrow();
    expect(() => roundDownToIncrement('1', '0')).toThrow();
    expect(decimalsOf('1.25')).toBe(2);
  });
});
