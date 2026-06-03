# Lit Action full-stack benchmark: PR #388 without deferred ops

This records a local full-stack benchmark for the Lit Action reported in PR #388:

<https://gist.githubusercontent.com/GTC6244/a9bbcb02aedabb3b885e462047612667/raw/5c899ecc97e4c1cd22353e1113bc0bc352f80edf/gistfile1.txt>

The benchmark starts the local `local_test.sh` stack for two refs, creates an account and usage API key, then repeatedly calls `POST /core/v1/lit_action`. It records both full HTTP wall time and the action's internal `Lit.Actions.getPrivateKey` timing parsed from the response text.

## Command

```bash
export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH"
export RUSTUP_TOOLCHAIN=1.92
export CARGO_BUILD_JOBS=1

scripts/bench_lit_action_full_stack.sh origin/main pr-388-no-deferred 10
```

## Refs

- before: `origin/main` = `06b12e5c38802bc8389f5b5804b7718f6f16621f`
- after: `pr-388-no-deferred` = `b5a3e0efe1eb01ab10b0e5b3a211e664e611849c`

## Results, 10 measured iterations

| Metric | main | PR #388 + no deferred ops | Delta |
|---|---:|---:|---:|
| wall mean | 70.27 ms | 58.48 ms | -16.8% |
| wall p50 | 70.99 ms | 58.08 ms | -18.2% |
| `getPrivateKey` mean | 5.30 ms | 5.53 ms | +4.2% |
| `getPrivateKey` p50 | 5.45 ms | 5.22 ms | -4.3% |
| errors | 0 | 0 | — |

Compared with PR #388 as originally tested (`#[op2(async(deferred), fast, reentrant)]` on the remote async ops), removing `async(deferred), fast` brings the `getPrivateKey` timing back in line with main. In the earlier PR #388 run, `getPrivateKey` mean was about `8.49 ms`; with default async ops it is about `5.53 ms`.

Raw JSON outputs are checked in next to this file:

- `lit-action-full-stack-before-main.json`
- `lit-action-full-stack-after-pr388-no-deferred.json`
- `lit-action-full-stack-pr388-no-deferred-summary.json`
