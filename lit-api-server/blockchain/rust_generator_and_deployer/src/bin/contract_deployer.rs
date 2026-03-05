//! Deploy contracts from a folder of ABI JSON artifacts to a chain.
//!
//! Usage: deploy <network> <abis_folder> [secret]
//!
//! network: 0 = Anvil, 1 = Yellowstone, 2 = LitMainnet
//! secret: optional; if blank or omitted, uses the default Anvil dev secret.

use ethers::contract::ContractFactory;
use ethers::prelude::*;
use ethers::utils::hex::FromHex;

use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

const ANVIL_RPC: &str = "http://127.0.0.1:8545";
const ANVIL_CHAIN_ID: u64 = 31337;
const YELLOWSTONE_RPC: &str = "https://yellowstone-rpc.litprotocol.com";
const YELLOWSTONE_CHAIN_ID: u64 = 175188;
const BASE_SEPOLIA_RPC: &str = "https://sepolia.base.org";
const BASE_SEPOLIA_CHAIN_ID: u64 = 84532;
/// Default Anvil account #0 private key (well-known for local dev)
const DEFAULT_SECRET: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <network> <abis_folder> [secret]",
            args.first().unwrap_or(&"deploy".into())
        );
        eprintln!("  network   - 0 = Anvil, 1 = Yellowstone, 2 = Base Sepolia");
        eprintln!(
            "  abis_folder - folder containing contract artifact JSON files (abi + bytecode)"
        );
        eprintln!(
            "  secret    - optional; deployer private key (hex). If blank or omitted, uses default."
        );
        std::process::exit(1);
    }
    let network: u16 = match args[1].parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("network must be 0, 1, or 2 (got: {:?})", args[1]);
            std::process::exit(1);
        }
    };
    let (rpc_url, chain_id) = match network {
        0 => (ANVIL_RPC, ANVIL_CHAIN_ID),
        1 => (YELLOWSTONE_RPC, YELLOWSTONE_CHAIN_ID),
        2 => (BASE_SEPOLIA_RPC, BASE_SEPOLIA_CHAIN_ID),
        _ => {
            eprintln!("network must be 0 (Anvil), 1 (Yellowstone), or 2 (Base Sepolia)");
            std::process::exit(1);
        }
    };

    let abis_folder = args[2].trim_end_matches('/').to_string();
    let secret = args
        .get(3)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_SECRET);
    println!(
        "Deploying contracts from folder {} on chain {} with RPC URL {}",
        args[2], abis_folder, rpc_url
    );

    deploy_contracts(rpc_url, chain_id, &abis_folder, secret)
        .await
        .expect("Failed to deploy contracts");
    Ok(())
}

async fn deploy_contracts(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let provider = Provider::<Http>::try_from(rpc_url).expect("Failed to create provider");

    let wallet: LocalWallet = secret.parse::<LocalWallet>()?.with_chain_id(chain_id);
    let client = SignerMiddleware::new(provider, wallet);
    let client = std::sync::Arc::new(client);
    let mut abis = Vec::new();
    get_abis(abis_folder, &mut abis);
    deploy_abis(abis, client)
        .await
        .expect("Failed to deploy contracts");
    Ok(())
}

fn get_abis(abis_folder: &str, abis: &mut Vec<PathBuf>) {
    let dir = fs::read_dir(abis_folder)
        .unwrap_or_else(|_| panic!("Failed to read directory {:?}", abis_folder));
    for entry in dir.flatten() {
        if entry.file_type().unwrap().is_dir() {
            if entry.path().to_str().unwrap().ends_with("Facets") {
                continue;
            }
            get_abis(entry.path().to_str().unwrap(), abis);
            continue;
        }
        if entry.path().to_str().unwrap().ends_with("json") {
            abis.push(entry.path());
        }
    }
}

async fn deploy_abis(
    abis: Vec<PathBuf>,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for abi in abis {
        deploy_artifact(&abi, client.clone())
            .await
            .expect("Failed to deploy contract");
    }
    Ok(())
}

/// Read ABI and bytecode from a Hardhat/Foundry-style artifact JSON and deploy to the connected chain.
async fn deploy_artifact(
    path: &Path,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let contents = fs::read_to_string(path)?;
    let artifact: serde_json::Value = serde_json::from_str(&contents)?;

    let abi_value = artifact.get("abi").ok_or("artifact missing 'abi'")?;
    let abi = ethers::abi::Abi::load(abi_value.to_string().as_bytes())?;

    let bytecode_hex = artifact
        .get("bytecode")
        .and_then(|v| v.as_str())
        .or_else(|| {
            artifact
                .get("evm")
                .and_then(|evm| evm.get("bytecode"))
                .and_then(|bc| bc.get("object"))
                .and_then(|o| o.as_str())
        })
        .ok_or("artifact missing 'bytecode' and evm.bytecode.object")?;

    if bytecode_hex.is_empty() || bytecode_hex == "0x" {
        println!("Skipping {} (no bytecode)", name);
        return Ok(());
    }

    let bytecode = Bytes::from_hex(bytecode_hex)?;
    let factory = ContractFactory::new(abi, bytecode, client);
    let deployer = factory.deploy(())?.legacy();
    println!(
        "Deploying {} with bytecode size {}.",
        name,
        bytecode_hex.len()
    );
    let (contract, _receipt) = deployer.send_with_receipt().await?;
    println!("Deployed {} -> {:?}", name, contract.address());
    Ok(())
}
