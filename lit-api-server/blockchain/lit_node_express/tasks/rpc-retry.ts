// Public Base RPCs cap eth_getLogs ranges (2,000 blocks on mainnet.base.org)
// and rate-limit bursts. Retry transient failures with backoff so a long event
// scan does not die halfway. Range-cap errors are not transient and surface at
// once with a hint to lower --chunk-size.
export async function withRetry<T>(
  label: string,
  fn: () => Promise<T>,
  attempts = 6
): Promise<T> {
  let delay = 500;
  for (let i = 1; ; i++) {
    try {
      return await fn();
    } catch (err) {
      const msg = (err as Error).message || String(err);
      if (/limited to a [\d,]+ range|block range/i.test(msg)) {
        throw new Error(`${label}: ${msg} — lower --chunk-size`);
      }
      if (i >= attempts) throw err;
      process.stderr.write(
        `\n  ${label} failed (${msg.slice(0, 80)}); retry ${i}/${attempts - 1} in ${delay}ms`
      );
      await new Promise((r) => setTimeout(r, delay));
      delay = Math.min(delay * 2, 10_000);
    }
  }
}
