# Run from blockchain/swaps/ (directory containing this MakeFile).
# Paths artifacts/contracts and ../../src/... are relative to this directory.

RUST_PROJECT := ../rust_generator_and_deployer
GENERATOR_BIN := $(RUST_PROJECT)/target/debug/contract_generator
DEPLOYER_BIN := $(RUST_PROJECT)/target/debug/contract_deployer

# 1. Compile Solidity contracts with Hardhat
compile:
	npx hardhat compile

# 2. Generate Rust bindings from artifacts. Args: input_folder, output_folder
generate: compile
	cd $(RUST_PROJECT) && cargo build --bin contract_generator
	$(GENERATOR_BIN) artifacts/contracts ../../src/accounts/contracts/

# 3. Deploy contracts. Args: network (1 = Yellowstone), abis_folder
deploy: generate
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) 0 artifacts/contracts
