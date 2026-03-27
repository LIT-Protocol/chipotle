use super::deploy::{deploy_artifact, deploy_contract};
use crate::diamond::c_diamond_cut_facet::FacetCut;
use crate::diamond::c_diamond_cut_facet::{
    DIAMONDCUTFACET_ABI, DIAMONDCUTFACET_BYTECODE, DiamondCutFacet,
};
use crate::diamond::c_diamond_loupe_facet::{
    DIAMONDLOUPEFACET_ABI, DIAMONDLOUPEFACET_BYTECODE, DiamondLoupeFacet,
};
use crate::diamond::c_ownership_facet::{OWNERSHIPFACET_ABI, OWNERSHIPFACET_BYTECODE};
use ethers::abi::FunctionExt;
use ethers::core::types::Address;
use ethers::prelude::*;
use std::path::Path;

pub async fn deploy_facet_from_json(
    abis_folder: &str,
    json_path: &str,
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
) -> Result<
    Contract<SignerMiddleware<Provider<Http>, LocalWallet>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let json_path = format!("{}/{}", abis_folder, json_path);
    let path = Path::new(&json_path);
    let facet = deploy_artifact(path, client.clone(), ()).await?;
    Ok(facet)
}

pub fn get_facet_cuts(
    contract: &Contract<SignerMiddleware<Provider<Http>, LocalWallet>>,
    existing_selectors: &[[u8; 4]],
    display: bool,
) -> Vec<FacetCut> {
    let mut facet_cuts = Vec::new();

    let selectors: Vec<[u8; 4]> = contract
        .abi()
        .functions()
        .map(|function| function.selector())
        .collect();

    let replace_selectors: Vec<[u8; 4]> = selectors
        .iter()
        .filter(|selector| existing_selectors.contains(selector))
        .cloned()
        .collect();

    if !replace_selectors.is_empty() {
        facet_cuts.push(FacetCut {
            facet_address: contract.address(),
            action: FacetCutAction::Replace as u8,
            function_selectors: replace_selectors,
        });
    }

    let add_selectors: Vec<[u8; 4]> = selectors
        .iter()
        .filter(|selector| !existing_selectors.contains(selector))
        .cloned()
        .collect();
    if !add_selectors.is_empty() {
        facet_cuts.push(FacetCut {
            facet_address: contract.address(),
            action: FacetCutAction::Add as u8,
            function_selectors: add_selectors,
        });
    }

    if display {
        for facet_cut in &facet_cuts {
            println!(
                "Facet cut {:?}, action: {} {} function(s).",
                facet_cut.facet_address,
                action_to_string(facet_cut.action),
                facet_cut.function_selectors.len()
            );
        }
    }
    facet_cuts
}

pub enum FacetCutAction {
    Add = 0,
    Replace = 1,
    Remove = 2,
}

/// Shared struct holding the results of deploying facets and building diamond cut data.
pub struct DiamondUpdateData {
    pub facet_cuts: Vec<FacetCut>,
    pub init_address: Address,
    pub init_calldata: Bytes,
    pub facets_deployed: std::collections::HashMap<String, Address>,
}

/// Deploy new facet versions and build the diamondCut calldata for an existing diamond.
/// This is the shared logic used by both `update_diamond` (sends tx directly) and
/// `propose_update_diamond` (writes JSON proposal).
async fn build_diamond_update(
    client: std::sync::Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    abis_folder: &str,
    diamond_address: Address,
) -> Result<DiamondUpdateData, Box<dyn std::error::Error + Send + Sync>> {
    let diamond_loupe_facet = DiamondLoupeFacet::new(diamond_address, client.clone());
    let facet_addresses = diamond_loupe_facet.facet_addresses().call().await?;

    let mut existing_selectors: Vec<[u8; 4]> = Vec::new();
    for facet_address in &facet_addresses {
        let selectors = diamond_loupe_facet
            .facet_function_selectors(*facet_address)
            .call()
            .await?;
        existing_selectors.extend(selectors);
    }

    println!(
        "Contract {:?} (before update) has {} facets with {} function selectors.",
        diamond_address,
        facet_addresses.len(),
        existing_selectors.len()
    );

    let mut facet_cuts = Vec::new();
    let mut facets_deployed = std::collections::HashMap::new();

    let api_config = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/APIConfigFacet.sol/APIConfigFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&api_config, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);
    facets_deployed.insert("APIConfigFacet".to_string(), api_config.address());

    let billing_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/BillingFacet.sol/BillingFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&billing_facet, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);
    facets_deployed.insert("BillingFacet".to_string(), billing_facet.address());

    let views_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/ViewsFacet.sol/ViewsFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&views_facet, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);
    facets_deployed.insert("ViewsFacet".to_string(), views_facet.address());

    let writes_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/WritesFacet.sol/WritesFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&writes_facet, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);
    facets_deployed.insert("WritesFacet".to_string(), writes_facet.address());

    let ownership_facet = deploy_contract(
        "OwnershipFacet",
        (*OWNERSHIPFACET_ABI).clone(),
        OWNERSHIPFACET_BYTECODE.clone(),
        client.clone(),
        (),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&ownership_facet, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);
    facets_deployed.insert("OwnershipFacet".to_string(), ownership_facet.address());

    let diamond_init = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/DiamondInit.sol/DiamondInit.json",
        client.clone(),
    )
    .await?;
    let init = diamond_init
        .abi()
        .functions_by_name("init")
        .unwrap()
        .first()
        .unwrap()
        .selector();

    Ok(DiamondUpdateData {
        facet_cuts,
        init_address: diamond_init.address(),
        init_calldata: Bytes::from(init),
        facets_deployed,
    })
}

pub async fn deploy_diamond(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let provider = Provider::<Http>::try_from(rpc_url).expect("Failed to create provider");

    let wallet: LocalWallet = secret.parse::<LocalWallet>()?.with_chain_id(chain_id);
    let client = SignerMiddleware::new(provider, wallet);
    let client = std::sync::Arc::new(client);
    let existing_selectors = &Vec::<[u8; 4]>::new();
    let display = false;

    let mut facet_cuts = Vec::new();

    let diamond_init = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/DiamondInit.sol/DiamondInit.json",
        client.clone(),
    )
    .await?;
    // get the init function from the diamond_init contract
    let init = diamond_init
        .abi()
        .functions_by_name("init")
        .unwrap()
        .first()
        .unwrap()
        .selector();

    let diamond_cut = deploy_contract(
        "DiamondCutFacet",
        (*DIAMONDCUTFACET_ABI).clone(),
        DIAMONDCUTFACET_BYTECODE.clone(),
        client.clone(),
        (),
    )
    .await?;

    facet_cuts.extend(get_facet_cuts(&diamond_cut, existing_selectors, display));

    let diamond_loupe = deploy_contract(
        "DiamondLoupeFacet",
        (*DIAMONDLOUPEFACET_ABI).clone(),
        DIAMONDLOUPEFACET_BYTECODE.clone(),
        client.clone(),
        (),
    )
    .await?;
    facet_cuts.extend(get_facet_cuts(&diamond_loupe, existing_selectors, display));

    let ownership_facet = deploy_contract(
        "OwnershipFacet",
        (*OWNERSHIPFACET_ABI).clone(),
        OWNERSHIPFACET_BYTECODE.clone(),
        client.clone(),
        (),
    )
    .await?;
    facet_cuts.extend(get_facet_cuts(
        &ownership_facet,
        existing_selectors,
        display,
    ));

    let api_config = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/APIConfigFacet.sol/APIConfigFacet.json",
        client.clone(),
    )
    .await?;
    facet_cuts.extend(get_facet_cuts(&api_config, existing_selectors, display));

    let billing_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/BillingFacet.sol/BillingFacet.json",
        client.clone(),
    )
    .await?;
    facet_cuts.extend(get_facet_cuts(&billing_facet, existing_selectors, display));

    let views_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/ViewsFacet.sol/ViewsFacet.json",
        client.clone(),
    )
    .await?;
    facet_cuts.extend(get_facet_cuts(&views_facet, existing_selectors, display));

    let writes_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/WritesFacet.sol/WritesFacet.json",
        client.clone(),
    )
    .await?;
    facet_cuts.extend(get_facet_cuts(&writes_facet, existing_selectors, display));

    let args = (
        client.address(),
        facet_cuts,
        diamond_init.address(),
        Bytes::from(init),
    );

    let account_config_path = format!("{}/AccountConfig.sol/AccountConfig.json", abis_folder);
    let account_config_path = Path::new(&account_config_path);
    let account_config = deploy_artifact(account_config_path, client.clone(), args).await;
    if let Err(e) = account_config {
        eprintln!("Failed to deploy AccountConfig: {:?}", e);
        return Err(e);
    }
    Ok(())
}

pub async fn update_diamond(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
    diamond_address: Address,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let provider = Provider::<Http>::try_from(rpc_url).expect("Failed to create provider");
    let wallet: LocalWallet = secret.parse::<LocalWallet>()?.with_chain_id(chain_id);
    let client = SignerMiddleware::new(provider, wallet);
    let client = std::sync::Arc::new(client);

    let data = build_diamond_update(client.clone(), abis_folder, diamond_address).await?;

    let diamond_cut_facet = DiamondCutFacet::new(diamond_address, client.clone());
    print!("Cutting diamond with init  {:?} ...", data.init_calldata);
    let tx = diamond_cut_facet.diamond_cut(data.facet_cuts, data.init_address, data.init_calldata);
    let pending_tx = tx.send().await?;
    let _receipt = pending_tx.await?;
    println!("Diamond contract updated!");

    let diamond_loupe_facet = DiamondLoupeFacet::new(diamond_address, client.clone());
    let facet_addresses = diamond_loupe_facet.facet_addresses().call().await?;
    let mut new_selectors: Vec<[u8; 4]> = Vec::new();
    for facet_address in &facet_addresses {
        let selectors = diamond_loupe_facet
            .facet_function_selectors(*facet_address)
            .call()
            .await?;
        new_selectors.extend(selectors);
    }
    println!(
        "Contract {:?} (after update) has {} facets with {} function selectors.",
        diamond_address,
        facet_addresses.len(),
        new_selectors.len()
    );

    Ok(())
}

/// Deploy facets and write a JSON proposal file instead of sending the diamondCut tx.
/// The proposal file can be consumed by the Hardhat `propose-diamond-cut` task to
/// submit through a Safe multisig.
pub async fn propose_update_diamond(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
    diamond_address: Address,
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let provider = Provider::<Http>::try_from(rpc_url).expect("Failed to create provider");
    let wallet: LocalWallet = secret.parse::<LocalWallet>()?.with_chain_id(chain_id);
    let client = SignerMiddleware::new(provider, wallet);
    let client = std::sync::Arc::new(client);

    let data = build_diamond_update(client.clone(), abis_folder, diamond_address).await?;

    // Build the encoded diamondCut calldata using the contract binding
    let diamond_cut_facet = DiamondCutFacet::new(diamond_address, client.clone());
    let tx = diamond_cut_facet.diamond_cut(data.facet_cuts, data.init_address, data.init_calldata);
    let calldata = tx.calldata().expect("Failed to encode diamondCut calldata");

    // Build facets_deployed as a JSON object
    let facets_json: serde_json::Map<String, serde_json::Value> = data
        .facets_deployed
        .iter()
        .map(|(name, addr)| {
            (
                name.clone(),
                serde_json::Value::String(format!("{:?}", addr)),
            )
        })
        .collect();

    let proposal = serde_json::json!({
        "to": format!("{:?}", diamond_address),
        "data": format!("0x{}", hex::encode(&calldata)),
        "value": "0",
        "operation": 0,
        "facets_deployed": facets_json
    });

    let proposal_str = serde_json::to_string_pretty(&proposal)?;
    std::fs::write(output_path, &proposal_str)?;
    println!("Diamond cut proposal written to {}", output_path);
    println!("{}", proposal_str);

    Ok(())
}

fn action_to_string(action: u8) -> String {
    match action {
        0 => String::from("Add"),
        1 => String::from("Replace"),
        2 => String::from("Remove"),
        _ => String::from("Unknown"),
    }
}
