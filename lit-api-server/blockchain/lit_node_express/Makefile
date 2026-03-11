# Run from blockchain/lit_node_express/ (directory containing this Makefile).
# Paths artifacts/contracts and ../../src/... are relative to this directory.

RUST_PROJECT := ../rust_generator_and_deployer
GENERATOR_BIN := $(RUST_PROJECT)/target/debug/contract_generator
DEPLOYER_BIN := $(RUST_PROJECT)/target/debug/contract_deployer
DEPLOYER_SECRET := 0xd9884c772d9e85843d36fc4b18972068efc02ec6f50924725d2cc39ecefcd8ef
BASE_DEPLOYER_SECRET := 0x747ea873f95c08f90634f496bbdbf53b852e297704499dfd3494e3046d44f016
ANVIL_SECRET := 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80

clean:
	npx hardhat clean
# 1. Compile Solidity contracts with Hardhat
compile:
	npx hardhat clean
	npx hardhat compile
	node generate-diamond-abi.mjs

#1.1 generate and compile
generate_and_compile: generate
    npx hardhat clean
	npx hardhat compile

# 2. Generate Rust bindings from artifacts. Args: input_folder, output_folder
generate: compile
	cd $(RUST_PROJECT) && cargo build --bin contract_generator
	$(GENERATOR_BIN) artifacts/contracts ../../src/accounts/contracts/

# 3. Deploy contracts to Base mainnet. Args: network (3 = Base), abis_folder
deploy_base: clean generate
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) 3 artifacts/contracts $(BASE_DEPLOYER_SECRET)

# 4. Deploy contracts to Base Sepolia. Args: network (2 = Base Sepolia), abis_folder
deploy_sepolia: clean generate
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) 2 artifacts/contracts $(DEPLOYER_SECRET)

# 5. Deploy contracts to Anvil. Args: network (0 = Anvil), abis_folder
deploy_anvil: clean generate
	cd $(RUST_PROJECT) && cargo build --bin contract_deployer
	$(DEPLOYER_BIN) 0 artifacts/contracts $(ANVIL_SECRET)