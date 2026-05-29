# Uptime Insurance

**Parametric "productivity insurance": get paid in ETH, automatically, when a
service you depend on goes down.** A [lit-triggers](https://triggers.litprotocol.com)
schedule trigger fires a Lit Action every minute; the action checks a status
page and, if the service is in a major/critical incident, pays a fixed amount
to the policyholder — signed and broadcast by the action's own wallet, a key no
human holds. That wallet's balance *is* the insurance pool.

## Why this is interesting

Parametric insurance normally needs three trusted parties: an **oracle** for the
data, a **keeper** to run the check, and a **multisig** to release funds. This
collapses all three into one trust-minimized unit — the same content-addressed
action reads the status *and* signs the payout. No admin can rug the pool, and
the oracle can't be bribed independently of the payout, because they're the same
code. Edit the action by a byte and its CID, its pool-wallet address, and its
authorization all change.

```
   cron tick            lit-triggers (schedule)     Lit network            payout
   ─────────            ───────────────────────     ───────────            ──────
   every minute  ──►  run: main(params)
                         │ fetch status summary.json
                         │ indicator in {major,critical}?
                         │   no  → no-op
                         │   yes → sign + send ETH ───────────────────────►  policyholder
                         │          from the pool (action wallet)             (+payoutWei)
                         ▼
                      run history
```

There is **no contract** — the pool is the action wallet's ETH balance and a
payout is a plain transfer. (A production version might use a pool contract that
collects premiums and caps payouts; see below.)

## Files

| Path | Purpose |
| --- | --- |
| `action/uptimeInsurance.js` | The Lit Action: check the status page, pay out if the service is down. |
| `scripts/setup.js` | One-shot: action CID → group → scoped key → derive + fund the pool wallet → authorize lit-triggers → create the schedule trigger. |
| `scripts/claim.js` | Watches for the next scheduled payout, shows the balance delta, then disables the trigger so the demo pool stops draining. |
| `scripts/_env.js` | Tiny shared `.env` reader / upserter. |

## Walkthrough

```bash
cp .env.example .env      # set LIT_API_KEY (master) + DEPLOYER_PRIVATE_KEY
npm install
npm run setup             # opens a browser — click "Authorize agent"
npm run claim             # watch a payout fire on the next tick
```

`setup` runs the schedule trigger every minute and (for the demo) forces the
"down" branch so a payout reliably fires. `claim` records the policyholder
balance, waits for the next run, prints the payout tx + balance delta, then
disables the trigger.

Expected `claim` output:

```
Policyholder 0x…
  balance before: 0.1041 ETH
Pool 0x…: 0.001 ETH
Waiting for the next scheduled run (cron tick)...
  run status: success
  action result: {"ok":true,"indicator":"critical","paid":true,"pool":"0x…","to":"0x…","amount_wei":"200000000000000","txHash":"0x…"}
  payout tx: 0x…
  balance after:  0.1043 ETH
  delta:          +0.0002 ETH
✓ Parametric payout executed autonomously by the keyless pool wallet.
Disabled the trigger (demo cleanup).
```

For a real policy, set `DEMO_FORCE_DOWN=false` so the actual `status.indicator`
drives payouts, and point `STATUS_URL` at the service you depend on.

## Production considerations

- **Debounce.** This pays on the current indicator. A real policy should require
  an incident to persist (e.g. N consecutive failures, or an incident open for
  longer than a threshold) — which needs state across runs or a status feed that
  reports incident duration. The action is stateless per run.
- **Pool accounting.** Here the pool is an unstructured wallet balance. A
  production version would use a contract that collects premiums, tracks
  policies, and caps total payouts per incident / per period.
- **Pool gas.** The pool wallet pays its own gas to send the payout; keep it
  funded above payout + gas.
- **Replay / double-pay.** Every qualifying tick pays out. Add a per-incident
  once-guard (record the incident id and skip if already paid) so a multi-tick
  outage doesn't drain the pool.
