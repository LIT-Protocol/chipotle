# Run from blockchain/lit_node_express/ (directory containing this Makefile).
# Paths artifacts/contracts and ../../src/... are relative to this directory.

RUST_PROJECT := ../rust_generator_and_deployer
GENERATOR_BIN := $(RUST_PROJECT)/target/debug/contract_generator
DEPLOYER_BIN := $(RUST_PROJECT)/target/debug/contract_deployer
DEPLOYER_SECRET ?= $(error DEPLOYER_SECRET is not set — export it as an env var)
ANVIL_SECRET ?= 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80

# 1. Compile Solidity contracts with Hardhat
compile:
	npm i
	npx hardhat clean
	npx hardhat compile

facade:
	node generate-diamond-abi.mjs

# 2. Generate Rust bindings from artifacts.
generate: compile facade
	cd $(RUST_PROJECT) && cargo build --bin contract_generator
	$(GENERATOR_BIN) --abifolder=artifacts/contracts --outputfolder=../../src/accounts/contracts/

# 3. Deploy contracts to Base mainnet.
deploy_base: compile
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) --action=deploy --network=base --abifolder=artifacts/contracts --secret=$(DEPLOYER_SECRET)

# 4. Deploy contracts to Base Sepolia.
deploy_sepolia: compile
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) --action=deploy --network=base-sepolia --abifolder=artifacts/contracts --secret=$(DEPLOYER_SECRET)

# 5. Deploy contracts to Anvil.
deploy_anvil: compile
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) --action=deploy --network=anvil --abifolder=artifacts/contracts --secret=$(ANVIL_SECRET)

update_anvil: compile
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) --action=update --network=anvil --abifolder=artifacts/contracts --secret=$(ANVIL_SECRET) --address=$(address)

update_sepolia: compile
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) --action=update --network=base-sepolia --abifolder=artifacts/contracts --secret=$(DEPLOYER_SECRET) --address=$(address)