# Run from the e2e/ directory.
#
# This wraps the repo's `local_test.sh` (anvil + dstack-simulator + contracts +
# lit-api-server + lit-actions + lit-static) and runs Playwright against it.
#
# Prereqs handled by local_test.sh itself:
#   - foundry on PATH (anvil/forge/cast)
#   - dstack-simulator built at $SIMULATOR_DIR (or ~/GitHub/dstack/sdk/simulator)
#   - docker for jaeger
#   - static-web-server (brew install static-web-server)
#
# WC_PROJECT_ID must be exported for WalletConnect tests (free at
# https://cloud.reown.com — same key the dashboard uses).

SHELL := /usr/bin/env bash

ANVIL_PORT       ?= 8545
ANVIL_RPC_URL    ?= http://127.0.0.1:$(ANVIL_PORT)
DASHBOARD_URL    ?= http://localhost:8080/dapps/dashboard/
API_BASE_URL     ?= http://localhost:8000
NODE_CONFIG_PATH ?= ../lit-api-server/NodeConfig.toml

export ANVIL_RPC_URL DASHBOARD_URL API_BASE_URL NODE_CONFIG_PATH

.PHONY: help
help:
	@echo "Targets:"
	@echo "  install         install node deps + playwright browsers"
	@echo "  up              start the full local stack (./local_test.sh in background)"
	@echo "  up-no-code      start anvil + dstack + jaeger only (run cargo services yourself)"
	@echo "  down            stop the background local stack"
	@echo "  test            run the full Playwright suite (assumes up)"
	@echo "  test-api        run only the API-mode suite (no wallets needed)"
	@echo "  test-eoa        run only the MetaMask/EOA suite"
	@echo "  test-wc         run only the WalletConnect suite"
	@echo "  test-flows      run only the parameterized flow suite"
	@echo "  typecheck       tsc --noEmit"
	@echo "  test-ci         up + test + down"

.PHONY: install
install:
	pnpm install
	pnpm exec playwright install --with-deps chromium

.PHONY: up
up:
	@pkill -f "local_test.sh" 2>/dev/null || true
	@(cd .. && ./local_test.sh &>/tmp/lit-local-stack.log & echo $$! > /tmp/lit-local-stack.pid)
	@npx -y wait-on $(API_BASE_URL) -t 180000
	@npx -y wait-on $(DASHBOARD_URL) -t 60000
	@echo "stack up — dashboard=$(DASHBOARD_URL) api=$(API_BASE_URL)"

.PHONY: up-no-code
up-no-code:
	@pkill -f "local_test.sh" 2>/dev/null || true
	@(cd .. && ./local_test.sh --no_code &>/tmp/lit-local-stack.log & echo $$! > /tmp/lit-local-stack.pid)
	@echo "anvil + dstack-simulator + jaeger started; start lit-api-server/lit-actions/static-web-server in your own terminals."

.PHONY: down
down:
	-@kill `cat /tmp/lit-local-stack.pid 2>/dev/null` 2>/dev/null || true
	-@pkill -f "local_test.sh" 2>/dev/null || true
	-@rm -f /tmp/lit-local-stack.pid

.PHONY: test
test:
	pnpm test

.PHONY: test-api
test-api:
	pnpm exec playwright test --project=api-mode

.PHONY: test-eoa
test-eoa:
	pnpm exec playwright test --project=eoa

.PHONY: test-wc
test-wc:
	pnpm exec playwright test --project=walletconnect

.PHONY: test-flows
test-flows:
	pnpm exec playwright test --project=flows-api --project=flows-wc

.PHONY: typecheck
typecheck:
	pnpm exec tsc --noEmit

.PHONY: test-ci
test-ci:
	$(MAKE) up
	$(MAKE) test || ($(MAKE) down; exit 1)
	$(MAKE) down
