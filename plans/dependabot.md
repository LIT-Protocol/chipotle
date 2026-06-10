# Dependabot alert cleanup plan

Tracking issue surface: <https://github.com/LIT-Protocol/chipotle/security/dependabot>

## Snapshot (2026-06-09)

Before this work there were **274 open** Dependabot alerts: 231 npm + 43 rust.

| Bucket | Count | Disposition |
| --- | ---: | --- |
| npm under `examples/` | 144 | **Dismissed** (`not_used`) — see step 1 |
| npm first-party (`lit-api-server`, `lit-actions`, `lit-payments`, `e2e`) | 87 | Out of scope for this plan (Rust only) — see "Follow-ups" |
| rust | 43 | This plan, steps 2–3 |

After step 1 (already done): **130 open** (87 npm + 43 rust).

The 43 rust alerts split cleanly into **~25–27 fixable by lockfile bump** and **~16 already-documented blockers** that match the `deny.toml` ignore list and should be marked won't-fix.

---

## Step 1 — Keep `examples/` out of the scanners ✅ DONE

`examples/` is standalone demo/sample code (Lit Action examples, hardhat projects,
dashboards). It ships in no production artifact, so its transitive deps should not
generate alerts or gate PRs.

**Socket (PR-creation scan):** added `socket.yml` at repo root with
`projectIgnorePaths: ["examples", "examples/**"]`. Socket reads this on the next
PR; `examples/` manifests are excluded from analysis and PR alerts.

**Dependabot:** there is **no native per-path exclusion for security *alerts***.
`exclude-paths` in `dependabot.yml` only suppresses *version-update PRs*, not the
Security-tab alerts (dependabot/dependabot-core#14408, #7522). The maintainable
equivalent is to auto-dismiss alerts under `examples/`:

- `scripts/dismiss-examples-dependabot.sh` — dismisses every open alert whose
  `manifest_path` starts with `examples/`, reason `not_used`. Re-run it after
  adding a new example or when new example alerts appear. (`DRY_RUN=1` to preview.)
- Ran once on 2026-06-09: **144 alerts dismissed**, 0 remaining under `examples/`.

> Note: the default Actions `GITHUB_TOKEN` cannot dismiss Dependabot alerts, so
> this is a maintainer-run local script (needs a token with `security_events`
> write) rather than a scheduled workflow. If we later want it automated, add a
> PAT secret and wrap the script in a `schedule:` + `workflow_dispatch` workflow.

---

## Step 2 — Fix the Rust alerts we *can* fix (lockfile bumps) ✅ DONE (2026-06-09)

These are semver-compatible `Cargo.lock`-only bumps (no `Cargo.toml` changes) —
the same "Tier 1" pattern documented in `deny.toml`'s history. Once the lockfiles
no longer reference the vulnerable version, Dependabot auto-closes the alerts.

**Outcome:** openssl ×3, rand (lit-core), and rsa (lit-actions) all bumped —
**clears 26 alerts**. `tar` turned out to be deno-pinned (`deno_npm_cache` requires
`=0.4.45`) → stays in step 3. `cargo deny ... advisories sources` passes in all
three touched workspaces and the lockfiles resolve `--locked`.

### 2a. `openssl` → `0.10.80` — 24 alerts (highest priority) 🔴

Not in `deny.toml`'s ignore list — these 8 CVEs (all 2026, mostly High) were
published after the 2026-06-09 advisory snapshot. **Bumping also pre-empts a
future `cargo deny` / rust-ci failure** if/when these land in the RustSec DB.

Currently locked: lit-core `0.10.73`, lit-api-server `0.10.75`, lit-actions
`0.10.73`. All fixes land by `0.10.80` (semver-compatible 0.10.x).

```bash
# run in each workspace dir
for ws in lit-core lit-api-server lit-actions; do
  (cd "$ws" && cargo update -p openssl --precise 0.10.80)
done
```

Clears alerts: `#91 #92 #93 #94 #95 #96 #97 #98 #99 #100 #101 #102 #103 #104 #105
#125 #126 #127 #132 #133 #134 #135 #136 #138`
(CVE-2026-41676/41677/41678/41681/41898/42327/44662/45784 × 3 workspaces).

### 2b. `rand` → `0.8.6` in `lit-core` — 1 alert 🟡

`deny.toml` already bumped rand in lit-api-server/payments/generator, but missed
lit-core (still `0.8.5`). lit-core is not deno-pinned, so it can move.

```bash
(cd lit-core && cargo update -p rand@0.8.5 --precise 0.8.6)
```

Clears `#106` (GHSA-cq8v-f236-94qc / RUSTSEC-2026-0097).
> The lit-actions copy of this same advisory (`#90`) is deno-pinned → see step 3.

### 2c. `tar` → `0.4.46` in `lit-actions` — BLOCKED ❌

Attempted `cargo update -p tar --precise 0.4.46`; failed with
`failed to select a version for the requirement tar = "=0.4.45"`. The pin comes
from `deno_npm_cache` (`cargo tree -i tar@0.4.45`), so `#368` (GHSA-3pv8) is
deno-blocked and moves to step 3.

### 2d. `rsa` → `0.9.10` in `lit-actions` — 1 alert ✅

`#19` (CVE-2026-21895 / GHSA-9c48-w39g-hm26, panic on prime == 1) is a *different*
advisory from the Marvin one already ignored as RUSTSEC-2023-0071. Bumped `0.9.9`
→ `0.9.10` cleanly — deno did **not** hard-pin it. Clears `#19`.

```bash
(cd lit-actions && cargo update -p rsa --precise 0.9.10)
```

### After the bumps

1. `cargo deny check --config deny.toml advisories sources` in each touched
   workspace (per `.github/workflows/rust-ci.yml`) — must stay green.
2. Build/test the touched workspaces.
3. Add a `deny.toml` history entry dated 2026-06-XX noting the openssl/rand/tar
   bumps (mirroring the existing changelog style).
4. Open one PR with the lockfile changes. Dependabot auto-closes the alerts on merge.

---

## Step 3 — Mark the rest won't-fix (already documented blockers)

Every remaining rust alert maps 1:1 to an entry already in `deny.toml`'s `ignore`
list, held by an upstream we can't move without forking **deno** or git-pinning
**rocket**/**Alloy** (see `deny.toml`'s BLOCKER SUMMARY). They already pass
`cargo deny` via the ignore list; the Dependabot alerts are the duplicate surface.

Dismiss each with reason **`no_bandwidth`** (or `tolerable_risk` for the low-sev
ones), comment pointing at the `deny.toml` entry. These can be re-triaged when the
upstream dep bumps land.

| Alert(s) | Pkg | Advisory | `deny.toml` entry / blocker |
| --- | --- | --- | --- |
| `#26 #78 #79 #80 #82 #83 #84 #109 #110 #111` | rustls-webpki | GHSA-pwjx/965h/xgp8/82j2 | RUSTSEC-2026-0049/0098/0099/0104 — rocket 0.5.1 (rustls 0.21) + deno_tls (rustls 0.23.40) |
| `#128 #130` | hickory-proto | GHSA-3v94 | RUSTSEC-2026-0118 — no stable fix (0.26-beta only) + deno_net |
| `#129 #131` | hickory-proto | GHSA-q2qq | RUSTSEC-2026-0119 — deno_net hickory 0.25 pin |
| `#21` | time | GHSA-r6v5 / CVE-2026-25727 | RUSTSEC-2026-0009 — deno_ast/swc serde pin |
| `#90` | rand | GHSA-cq8v | RUSTSEC-2026-0097 — deno's rand `=0.8.5` (lit-actions only) |
| `#368` | tar | GHSA-3pv8 | `deno_npm_cache` requires `=0.4.45` (deno-pinned) |

> Verify the `lit-api-server` hickory source (`#130 #131`): `deny.toml` attributes
> hickory to deno (lit-actions) only. If lit-api-server pulls hickory via a
> non-deno edge (Alloy/reqwest), check whether `cargo update -p hickory-proto`
> can reach a fixed line there before dismissing — 0119 is fixed in ≥0.26.1, but
> 0118 has no stable fix regardless.

There is no bulk "dismiss by package" API; dismiss per alert number, e.g.:

```bash
gh api --method PATCH /repos/LIT-Protocol/chipotle/dependabot/alerts/<N> \
  -f state=dismissed -f dismissed_reason=no_bandwidth \
  -f dismissed_comment="Blocked upstream (deno/rocket/Alloy); see deny.toml ignore entry RUSTSEC-XXXX-XXXX."
```

(Or extend `scripts/dismiss-examples-dependabot.sh` with a by-alert-number mode.)

---

## End state

- `examples/`: 0 alerts (auto-dismiss script + `socket.yml`).
- Rust fixed: **26 alerts** closed by lockfile bumps (openssl ×24, rand ×1, rsa ×1).
- Rust blocked: **17 alerts** to dismiss `no_bandwidth`, each traceable to a
  `deny.toml` ignore entry; revisit when deno/rocket/Alloy ship dep bumps (the
  `deny.toml` "upstream watch" already tracks this).

## Follow-ups (out of scope for this Rust-only plan)

- **87 first-party npm alerts** remain: `lit-api-server/blockchain/lit_node_express`
  (39), `e2e/pnpm-lock.yaml` (27), `lit-actions/package-lock.json` (12),
  `lit-payments/contracts/package-lock.json` (9). Worth a separate pass —
  `npm audit fix` / `pnpm update` per project — but these gate real shipped code
  so they need actual testing, not just lockfile bumps.
