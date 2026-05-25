use alloy::contract::{ContractInstance, Interface};
use alloy::dyn_abi::DynSolValue;
use alloy::json_abi::JsonAbi;
use alloy::network::{Ethereum, TransactionBuilder};
use alloy::primitives::{Address, Bytes};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::{Signer, local::PrivateKeySigner};

use std::fs;
use std::path::{Path, PathBuf};

pub type SigningProvider = DynProvider<Ethereum>;
pub type DeployedContract = ContractInstance<SigningProvider>;

#[allow(dead_code)]
pub async fn deploy_contracts(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = signer_provider(rpc_url, chain_id, secret)?;
    let mut abis = Vec::new();
    get_abis(abis_folder, &mut abis, false);
    deploy_abis(abis, client).await?;
    Ok(())
}

pub fn signer_provider(
    rpc_url: &str,
    chain_id: u64,
    secret: &str,
) -> Result<SigningProvider, Box<dyn std::error::Error + Send + Sync>> {
    let wallet = secret
        .parse::<PrivateKeySigner>()?
        .with_chain_id(Some(chain_id));
    Ok(ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.parse()?)
        .erased())
}

pub fn signer_address(secret: &str) -> Result<Address, Box<dyn std::error::Error + Send + Sync>> {
    Ok(secret.parse::<PrivateKeySigner>()?.address())
}

pub fn get_abis(abis_folder: &str, abis: &mut Vec<PathBuf>, is_facet: bool) {
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
            abis.push(entry.path());
        }
    }
}

pub async fn deploy_abis(
    abis: Vec<PathBuf>,
    client: SigningProvider,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for abi in abis {
        deploy_artifact(&abi, client.clone(), Bytes::new()).await?;
    }
    Ok(())
}

/// Read ABI and bytecode from a Hardhat/Foundry-style artifact JSON and deploy to the connected chain.
pub async fn deploy_artifact(
    path: &Path,
    client: SigningProvider,
    constructor_args: Bytes,
) -> Result<DeployedContract, Box<dyn std::error::Error + Send + Sync>> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let contents = fs::read_to_string(path)?;
    deploy_artifact_json(name, &contents, client, constructor_args).await
}

/// Deploy a Hardhat/Foundry-style artifact JSON that has already been loaded.
pub async fn deploy_artifact_json(
    name: &str,
    contents: &str,
    client: SigningProvider,
    constructor_args: Bytes,
) -> Result<DeployedContract, Box<dyn std::error::Error + Send + Sync>> {
    let artifact: serde_json::Value = serde_json::from_str(contents)?;

    let abi_value = match artifact.get("abi") {
        Some(abi) => abi,
        None => {
            println!("Skipping {} (no abi)", name);
            return Err(Box::new(std::io::Error::other("No abi found")));
        }
    };
    let abi: JsonAbi = serde_json::from_value(abi_value.clone())?;

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
        return Err(Box::new(std::io::Error::other("No bytecode found")));
    }

    let bytecode = alloy::hex::decode(bytecode_hex.trim_start_matches("0x"))?;
    deploy_contract(name, abi, Bytes::from(bytecode), client, constructor_args).await
}

pub async fn deploy_contract(
    name: &str,
    abi: JsonAbi,
    bytecode: Bytes,
    client: SigningProvider,
    constructor_args: Bytes,
) -> Result<DeployedContract, Box<dyn std::error::Error + Send + Sync>> {
    print!("Deploying contract {} ...", name);
    let deploy_data = if constructor_args.is_empty() {
        bytecode
    } else {
        let mut data = bytecode.to_vec();
        data.extend_from_slice(&constructor_args);
        Bytes::from(data)
    };

    let tx = legacy_tx_request().with_deploy_code(deploy_data);
    let receipt = client.send_transaction(tx).await?.get_receipt().await?;
    if !receipt.status() {
        return Err(format!("deployment transaction for {name} reverted").into());
    }
    let address = receipt
        .contract_address
        .ok_or("deployment receipt missing contract address")?;
    println!(" deployed to {:?}", address);
    Ok(ContractInstance::new(address, client, Interface::new(abi)))
}

pub fn artifact_abi(path: &Path) -> Result<JsonAbi, Box<dyn std::error::Error + Send + Sync>> {
    let contents = fs::read_to_string(path)?;
    artifact_abi_json(&contents)
}

pub fn artifact_abi_json(
    contents: &str,
) -> Result<JsonAbi, Box<dyn std::error::Error + Send + Sync>> {
    let artifact: serde_json::Value = serde_json::from_str(contents)?;
    let abi_value = artifact.get("abi").ok_or("artifact missing abi")?;
    Ok(serde_json::from_value(abi_value.clone())?)
}

pub fn legacy_tx_request() -> TransactionRequest {
    // Preserve the previous ethers deployer behavior: ContractFactory::deploy(...).legacy().
    TransactionRequest::default().transaction_type(0)
}

pub fn no_constructor_args() -> Bytes {
    Bytes::new()
}

#[allow(dead_code)]
pub fn encode_constructor_args(args: &[DynSolValue]) -> Bytes {
    Bytes::from(
        DynSolValue::Tuple(args.to_vec())
            .abi_encode_sequence()
            .expect("tuple constructor arguments should ABI encode"),
    )
}
