//! Deploy contracts from a folder of ABI JSON artifacts to a chain.
//!
//! Usage: deploy <network> <abis_folder> [secret]
//!
//! network: 0 = Anvil, 1 = Yellowstone, 2 = Base Sepolia, 3 = Base
//! secret: optional; if blank or omitted, uses the default Anvil dev secret.

use ethers::abi::FunctionExt;
use ethers::abi::Tokenize;
use ethers::contract::ContractFactory;
use ethers::prelude::*;
use ethers::utils::hex::FromHex;


use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

const ANVIL_RPC: &str = "http://127.0.0.1:8545";
const ANVIL_CHAIN_ID: u64 = 31337;
const YELLOWSTONE_RPC: &str = "https://yellowstone-rpc.litprotocol.com";
const YELLOWSTONE_CHAIN_ID: u64 = 175188;
const BASE_SEPOLIA_RPC: &str = "https://sepolia.base.org";
const BASE_SEPOLIA_CHAIN_ID: u64 = 84532;
const BASE_RPC: &str = "https://mainnet.base.org";
const BASE_CHAIN_ID: u64 = 8453;
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
        eprintln!("  network   - 0 = Anvil, 1 = Yellowstone, 2 = Base Sepolia, 3 = Base");
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
            eprintln!("network must be 0, 1, 2, or 3 (got: {:?})", args[1]);
            std::process::exit(1);
        }
    };
    let (rpc_url, chain_id) = match network {
        0 => (ANVIL_RPC, ANVIL_CHAIN_ID),
        1 => (YELLOWSTONE_RPC, YELLOWSTONE_CHAIN_ID),
        2 => (BASE_SEPOLIA_RPC, BASE_SEPOLIA_CHAIN_ID),
        3 => (BASE_RPC, BASE_CHAIN_ID),
        _ => {
            eprintln!("network must be 0 (Anvil), 1 (Yellowstone), 2 (Base Sepolia), or 3 (Base)");
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
    
    deploy_diamond(rpc_url, chain_id, &abis_folder, secret)
        .await
        .expect("Failed to deploy contracts");
    Ok(())
}

#[allow(dead_code)]
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
    get_abis(abis_folder, &mut abis, false);
    deploy_abis(abis, client)
        .await
        .expect("Failed to deploy contracts");
    Ok(())
}

fn get_abis(abis_folder: &str, abis: &mut Vec<PathBuf>, is_facet: bool) {
    let dir = fs::read_dir(abis_folder)
        .unwrap_or_else(|_| panic!("Failed to read directory {:?}", abis_folder));
    for entry in dir.flatten() {
        if entry.file_type().unwrap().is_dir() {
            let new_is_facet = match is_facet {
                true => true, 
                false => entry.path().to_str().unwrap().ends_with("Facets"),
            };
            get_abis(entry.path().to_str().unwrap(), abis, new_is_facet);
            continue;
        }
        if entry.path().to_str().unwrap().ends_with("json") {
            if entry.path().to_str().unwrap().ends_with("dbg.json") {
                continue;
            }
            // if is_facet && !abis_folder.ends_with("Facet") {
            //     continue;
            // }
            abis.push(entry.path());
        }
    }
}

async fn deploy_abis(
    abis: Vec<PathBuf>,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for abi in abis {
        let error_message = format!("Failed to deploy contract from {:?}", abi.to_str().unwrap());
        deploy_artifact(&abi, client.clone(), ())
            .await
            .expect(error_message.as_str());
    }
    Ok(())
}

/// Read ABI and bytecode from a Hardhat/Foundry-style artifact JSON and deploy to the connected chain.
async fn deploy_artifact<T: Tokenize>(
    path: &Path,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    args: T,
) -> Result<Contract<SignerMiddleware<Provider<Http>, LocalWallet>>, Box<dyn std::error::Error + Send + Sync>> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let contents = fs::read_to_string(path)?;
    let artifact: serde_json::Value = serde_json::from_str(&contents)?;

    let abi_value = match  artifact.get("abi") {
        Some(abi) => abi,
        None => {
            println!("Skipping {} (no abi)", name);
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "No abi found")));
        }
    };

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
        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "No bytecode found")));
    }

    let bytecode = Bytes::from_hex(bytecode_hex)?;
    let factory = ContractFactory::new(abi, bytecode, client);
    println!("Deploying {} ...", name);
    let deployer = factory.deploy(args)?.legacy();
    
    println!(
        "Deploying {} with bytecode size {}.",
        name,
        bytecode_hex.len()
    );
    let (contract, _receipt) = deployer.send_with_receipt().await?;
    println!("Deployed {} -> {:?}", name, contract.address());

    Ok(contract)
}

pub async fn deploy_facet_from_json(
    abis_folder: &str,
    json_path: &str,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
) -> Result<Contract<SignerMiddleware<Provider<Http>, LocalWallet>>, Box<dyn std::error::Error + Send + Sync>> {
    
    let json_path = format!("{}/{}", abis_folder, json_path);
    let path = Path::new(&json_path);
    let facet = deploy_artifact(path, client.clone(), ()).await?;
    Ok(facet)
}

pub fn get_facet_cut(action: FacetCutAction, contract: &Contract<SignerMiddleware<Provider<Http>, LocalWallet>>) -> FacetCut {
    let facet_cut = FacetCut {
        facet_address: contract.address(),
        action: action as u8,
        function_selectors: contract.abi().functions().map(|function| function.selector()).collect(),
    };
    return facet_cut;
}

pub enum FacetCutAction {
    Add = 0,
    Replace = 1,
    Remove = 2,
}


#[derive(Debug, Clone, EthAbiType, EthAbiCodec)]
pub struct FacetCut {
    pub facet_address: ::ethers::core::types::Address,
    pub action: u8,
    pub function_selectors: ::std::vec::Vec<[u8; 4]>,
}

async fn deploy_diamond(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let provider = Provider::<Http>::try_from(rpc_url).expect("Failed to create provider");

    let wallet: LocalWallet = secret.parse::<LocalWallet>()?.with_chain_id(chain_id);
    let client = SignerMiddleware::new(provider, wallet);
    let client = std::sync::Arc::new(client);
    

    let mut facet_cuts = Vec::new();

    let diamond_init = deploy_facet_from_json(abis_folder, "DiamondPattern/DiamondInit.sol/DiamondInit.json", client.clone()).await?;
    // get the init function from the diamond_init contract
    let init  = diamond_init.abi().functions_by_name("init").unwrap().first().unwrap().selector();

    let diamond_cut_facet = deploy_facet_from_json(abis_folder, "DiamondPattern/DiamondCutFacet.sol/DiamondCutFacet.json", client.clone()).await?;
    facet_cuts.push(get_facet_cut(FacetCutAction::Add, &diamond_cut_facet));

    let api_config_facet = deploy_facet_from_json(abis_folder, "AccountConfigFacets/APIConfigFacet.sol/APIConfigFacet.json", client.clone()).await?;
    facet_cuts.push(get_facet_cut(FacetCutAction::Add, &api_config_facet));

    let billing_facet = deploy_facet_from_json(abis_folder, "AccountConfigFacets/BillingFacet.sol/BillingFacet.json", client.clone()).await?;
    facet_cuts.push(get_facet_cut(FacetCutAction::Add, &billing_facet));

    let views_facet = deploy_facet_from_json(abis_folder, "AccountConfigFacets/ViewsFacet.sol/ViewsFacet.json", client.clone()).await?;
    facet_cuts.push(get_facet_cut(FacetCutAction::Add, &views_facet));

    let writes_facet = deploy_facet_from_json(abis_folder, "AccountConfigFacets/WritesFacet.sol/WritesFacet.json", client.clone()).await?;
    facet_cuts.push(get_facet_cut(FacetCutAction::Add, &writes_facet));

    let args = (client.address(), facet_cuts, diamond_init.address(), Bytes::from(init));

    let account_config_path = format!("{}/AccountConfig.sol/AccountConfig.json", abis_folder);
    let account_config_path = Path::new(&account_config_path);
    let account_config = deploy_artifact(account_config_path, client.clone(), args).await;
    if let Err(e) = account_config {
        eprintln!("Failed to deploy AccountConfig: {:?}", e);
        return Err(e.into());
    }

 
 
 
    Ok(())
}