# Agent Context: End-to-End Tests (Playwright / TypeScript)

## Purpose
UI-level Playwright coverage for the Chipotle dashboard (`lit-static/dapps/dashboard`).
Exercises the same functional surface as the k6 suite (chain config, account creation,
usage API keys, Lit Action execution, encrypt/decrypt) through the dashboard UI, plus
ChainSecured (wallet) flows. Projects: `api-mode`, `eoa`, `walletconnect`, `flows-*`.
Fixtures live in `fixtures/`, wallet cache setup in `wallet-setup/`, and the `Makefile`
wraps `../local_test.sh` to boot the stack.

## Running E2E tests

When you (an AI agent) run the Playwright E2E suite, you must produce and inspect
screenshots of what actually happened on screen — for passing tests too, not just
failures. A green exit code is not sufficient evidence that the UI did what the
test claims.

1. **Run with `AGENT_SCREENSHOTS=1`** (the stack must already be up — `make -C e2e up`):

   ```sh
   cd e2e && AGENT_SCREENSHOTS=1 pnpm test        # or pnpm test:api / test:eoa / test:wc / test:flows
   ```

   This flips `playwright.config.ts` to `screenshot: 'on'`, so every test —
   pass or fail — writes a PNG into `e2e/test-results/<test-name>/`.

2. **After the run, open the screenshots with the Read tool** and describe what is
   visible: dashboard state, wallet dialogs, error banners. Verify the screen
   matches what the test asserts, and flag anything that looks wrong even if the
   test passed.

3. **For step-by-step evidence, add `--trace on`.** Example:

   - `pnpm test -- --trace on` (or `pnpm test:api -- --trace on`, etc.)

   The trace zip in `e2e/test-results/` contains a screenshot of every action; point the user at
   `pnpm exec playwright show-trace <zip>` to replay it.

4. **Include the screenshot file paths in your final report** so the user can open
   them directly.

Note: the `api-mode` project is HTTP-only, so its screenshots are blank pages —
visual evidence matters for the `eoa`, `walletconnect`, and `flows-*` projects.
