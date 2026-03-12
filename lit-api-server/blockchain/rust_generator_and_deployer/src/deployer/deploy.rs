use ethers::abi::Tokenize;
use ethers::contract::ContractFactory;
use ethers::prelude::*;
use ethers::utils::hex::FromHex;

use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[allow(dead_code)]
pub async fn deploy_contracts(
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
            // if is_facet && !abis_folder.ends_with("Facet") {
            //     continue;
            // }
            abis.push(entry.path());
        }
    }
}

pub async fn deploy_abis(
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
pub async fn deploy_artifact<T: Tokenize>(
    path: &Path,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    args: T,
) -> Result<
    Contract<SignerMiddleware<Provider<Http>, LocalWallet>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let contents = fs::read_to_string(path)?;
    let artifact: serde_json::Value = serde_json::from_str(&contents)?;

    let abi_value = match artifact.get("abi") {
        Some(abi) => abi,
        None => {
            println!("Skipping {} (no abi)", name);
            return Err(Box::new(std::io::Error::other("No abi found")));
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
        return Err(Box::new(std::io::Error::other("No bytecode found")));
    }

    let bytecode = Bytes::from_hex(bytecode_hex)?;
    deploy_contract(name, abi, bytecode, client, args).await
}

pub async fn deploy_contract<T: Tokenize>(
    name: &str,
    abi: ethers::abi::Abi,
    bytecode: Bytes,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    args: T,
) -> Result<
    Contract<SignerMiddleware<Provider<Http>, LocalWallet>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    print!("Deploying contract {} ...", name);
    let factory = ContractFactory::new(abi, bytecode, client);
    let deployer = factory.deploy(args)?.legacy();
    let (contract, _receipt) = deployer.send_with_receipt().await?;
    println!(" deployed to {:?}", contract.address());
    Ok(contract)
}
