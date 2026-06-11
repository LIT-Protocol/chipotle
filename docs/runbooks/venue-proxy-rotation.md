# Runbook: rotating a venue connection's dedicated egress IP

Plan D4 (`plans/ccxt-venue-layer-and-email-approval.md`): every trade-enabled
CEX connection gets a **dedicated static proxy IP**, purchased from a proxy
provider, with the venue's API key allowlisted to exactly that IP. This
runbook covers replacing that IP — provider churn, provider ban, IP burn
(rate-limit pollution), or scheduled rotation.

The proxy abstraction is provider-agnostic: anything speaking
`http(s)://user:pass@host:port` CONNECT works (`Lit.Actions.proxiedFetch`,
lit-venues `proxy` config). TLS to the venue is end-to-end through the
tunnel — the provider sees SNI and timing, never API keys or payloads.

## Invariants (do not break)

1. **The venue key must never be usable from an IP the user's policy can't
   egress from.** During rotation the allowlist may briefly contain TWO IPs
   (old + new); it must never contain zero of ours while trading is expected,
   and the old IP must be removed once cutover is confirmed.
2. **Proxy credentials are secret material.** They live inside the sealed
   `venue-credentials-v1` blob (`egress.proxyUrl`) — resealing is part of the
   rotation, and the old credentials are dead after step 6.
3. One connection = one dedicated IP. Never share a trade-scope IP across
   tenants (per-IP venue rate limits make a noisy neighbor everyone's outage).

## Steps

1. **Provision the new IP** at the provider (region must satisfy the venue's
   geo policy — e.g. non-US for binance.com). Record provider, IP, region,
   credentials in the connection's metadata.
2. **Verify the new exit works** before touching anything live:
   `LIT_VENUES_PROXY=<new url> node lit-venues/scripts/verify-live.mjs`
   (public tier is enough; it proves CONNECT auth + geo).
3. **Add the new IP to the venue-side allowlist** (Binance: API Management →
   Edit restrictions; the key now lists old + new).
4. **Reseal credentials** with `egress.proxyUrl` pointing at the new proxy
   (connect-flow reseal: decrypt-in-TEE → swap proxyUrl → Encrypt). The
   connection record's rendered "allowlist instructions" must show the new IP.
5. **Confirm cutover**: run the connection's verification action (or the
   venue k6 spec with the connection's params) and check the venue's API-key
   last-used-IP report if available.
6. **Remove the old IP** from the venue allowlist, then release the old proxy
   IP at the provider. Credentials sealed in step 4 are now the only working
   egress for that key.

## Provider redundancy (GA requirement, plan D4)

Keep **two vetted providers** with live credentials at all times. A provider
outage is then a rotation (this runbook) per affected connection, not an
incident. Track per-provider health in the venue status feed; if a provider
is degraded for >30 min, rotate the highest-value connections first
(trade-scope before read-scope).

## Notes

- Hyperliquid connections do not use this runbook: no exchange-side IP
  allowlist exists and the signing key is PKP-native (plan D8); shared CVM
  egress is the default there.
- Read-only CEX connections ride shared CVM egress (no dedicated IP, nothing
  to rotate).
