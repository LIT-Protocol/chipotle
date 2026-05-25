use super::deploy::{
    DeployedContract, SigningProvider, artifact_abi_json, deploy_artifact, deploy_artifact_json,
    encode_constructor_args, legacy_tx_request, no_constructor_args, signer_address,
    signer_provider,
};
use crate::diamond::c_diamond_cut_facet::DIAMONDCUTFACET_JSON;
use crate::diamond::c_diamond_cut_facet::FacetCut;
use crate::diamond::c_diamond_loupe_facet::DIAMONDLOUPEFACET_JSON;
use crate::diamond::c_ownership_facet::OWNERSHIPFACET_JSON;
use alloy::dyn_abi::DynSolValue;
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, B256, Bytes, FixedBytes, U256};
use alloy::providers::Provider;
use std::path::Path;

pub async fn deploy_facet_from_json(
    abis_folder: &str,
    json_path: &str,
    client: SigningProvider,
) -> Result<DeployedContract, Box<dyn std::error::Error + Send + Sync>> {
    let json_path = format!("{}/{}", abis_folder, json_path);
    let path = Path::new(&json_path);
    let facet = deploy_artifact(path, client.clone(), no_constructor_args()).await?;
    Ok(facet)
}

async fn deploy_prebuilt_facet(
    name: &str,
    json: &str,
    client: SigningProvider,
) -> Result<DeployedContract, Box<dyn std::error::Error + Send + Sync>> {
    deploy_artifact_json(name, json, client, no_constructor_args()).await
}

pub fn get_facet_cuts(
    contract: &DeployedContract,
    existing_selectors: &[FixedBytes<4>],
    display: bool,
) -> Vec<FacetCut> {
    let mut facet_cuts = Vec::new();

    let selectors: Vec<FixedBytes<4>> = contract
        .abi()
        .functions()
        .map(|function| function.selector())
        .collect();

    let replace_selectors: Vec<FixedBytes<4>> = selectors
        .iter()
        .filter(|selector| existing_selectors.contains(selector))
        .cloned()
        .collect();

    if !replace_selectors.is_empty() {
        facet_cuts.push(FacetCut {
            facetAddress: *contract.address(),
            action: FacetCutAction::Replace as u8,
            functionSelectors: replace_selectors,
        });
    }

    let add_selectors: Vec<FixedBytes<4>> = selectors
        .iter()
        .filter(|selector| !existing_selectors.contains(selector))
        .cloned()
        .collect();
    if !add_selectors.is_empty() {
        facet_cuts.push(FacetCut {
            facetAddress: *contract.address(),
            action: FacetCutAction::Add as u8,
            functionSelectors: add_selectors,
        });
    }

    if display {
        for facet_cut in &facet_cuts {
            println!(
                "Facet cut {:?}, action: {} {} function(s).",
                facet_cut.facetAddress,
                action_to_string(facet_cut.action),
                facet_cut.functionSelectors.len()
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

async fn facet_addresses(
    client: &SigningProvider,
    diamond_address: Address,
) -> Result<Vec<Address>, Box<dyn std::error::Error + Send + Sync>> {
    let loupe = loupe_contract(client.clone(), diamond_address)?;
    let values = loupe.function("facetAddresses", &[])?.call().await?;
    let addresses = values
        .first()
        .and_then(|v| v.as_array())
        .ok_or("facetAddresses returned unexpected value")?
        .iter()
        .map(|v| v.as_address().ok_or("non-address facet"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(addresses)
}

async fn facet_function_selectors(
    client: &SigningProvider,
    diamond_address: Address,
    facet_address: Address,
) -> Result<Vec<FixedBytes<4>>, Box<dyn std::error::Error + Send + Sync>> {
    let loupe = loupe_contract(client.clone(), diamond_address)?;
    let values = loupe
        .function(
            "facetFunctionSelectors",
            &[DynSolValue::Address(facet_address)],
        )?
        .call()
        .await?;
    let selectors = values
        .first()
        .and_then(|v| v.as_array())
        .ok_or("facetFunctionSelectors returned unexpected value")?
        .iter()
        .map(dyn_value_to_selector)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(selectors)
}

fn loupe_contract(
    client: SigningProvider,
    diamond_address: Address,
) -> Result<DeployedContract, Box<dyn std::error::Error + Send + Sync>> {
    let abi = artifact_abi_json(DIAMONDLOUPEFACET_JSON)?;
    Ok(alloy::contract::ContractInstance::new(
        diamond_address,
        client,
        alloy::contract::Interface::new(abi),
    ))
}

fn diamond_cut_contract(
    client: SigningProvider,
    diamond_address: Address,
) -> Result<DeployedContract, Box<dyn std::error::Error + Send + Sync>> {
    let abi = artifact_abi_json(DIAMONDCUTFACET_JSON)?;
    Ok(alloy::contract::ContractInstance::new(
        diamond_address,
        client,
        alloy::contract::Interface::new(abi),
    ))
}

/// Deploy new facet versions and build the diamondCut calldata for an existing diamond.
async fn build_diamond_update(
    client: SigningProvider,
    abis_folder: &str,
    diamond_address: Address,
) -> Result<DiamondUpdateData, Box<dyn std::error::Error + Send + Sync>> {
    let facet_addresses = facet_addresses(&client, diamond_address).await?;

    let mut existing_selectors: Vec<FixedBytes<4>> = Vec::new();
    for facet_address in &facet_addresses {
        let selectors = facet_function_selectors(&client, diamond_address, *facet_address).await?;
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

    for (name, json_path) in [
        (
            "APIConfigFacet",
            "AccountConfigFacets/APIConfigFacet.sol/APIConfigFacet.json",
        ),
        (
            "BillingFacet",
            "AccountConfigFacets/BillingFacet.sol/BillingFacet.json",
        ),
        (
            "ViewsFacet",
            "AccountConfigFacets/ViewsFacet.sol/ViewsFacet.json",
        ),
        (
            "WritesFacet",
            "AccountConfigFacets/WritesFacet.sol/WritesFacet.json",
        ),
    ] {
        let facet = deploy_facet_from_json(abis_folder, json_path, client.clone()).await?;
        facet_cuts.extend(get_facet_cuts(&facet, &existing_selectors, true));
        facets_deployed.insert(name.to_string(), *facet.address());
    }

    let ownership_facet =
        deploy_prebuilt_facet("OwnershipFacet", OWNERSHIPFACET_JSON, client.clone()).await?;
    facet_cuts.extend(get_facet_cuts(&ownership_facet, &existing_selectors, true));
    facets_deployed.insert("OwnershipFacet".to_string(), *ownership_facet.address());

    let diamond_init = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/DiamondInit.sol/DiamondInit.json",
        client.clone(),
    )
    .await?;

    Ok(DiamondUpdateData {
        facet_cuts,
        init_address: *diamond_init.address(),
        init_calldata: init_calldata(&diamond_init)?,
        facets_deployed,
    })
}

pub async fn deploy_diamond(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = signer_provider(rpc_url, chain_id, secret)?;
    let owner = signer_address(secret)?;
    let existing_selectors = &Vec::<FixedBytes<4>>::new();
    let display = false;

    let mut facet_cuts = Vec::new();

    let diamond_init = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/DiamondInit.sol/DiamondInit.json",
        client.clone(),
    )
    .await?;

    for (name, json) in [
        ("DiamondCutFacet", DIAMONDCUTFACET_JSON),
        ("DiamondLoupeFacet", DIAMONDLOUPEFACET_JSON),
        ("OwnershipFacet", OWNERSHIPFACET_JSON),
    ] {
        let facet = deploy_prebuilt_facet(name, json, client.clone()).await?;
        println!("Prepared facet {name}");
        facet_cuts.extend(get_facet_cuts(&facet, existing_selectors, display));
    }

    for (name, json_path) in [
        (
            "APIConfigFacet",
            "AccountConfigFacets/APIConfigFacet.sol/APIConfigFacet.json",
        ),
        (
            "BillingFacet",
            "AccountConfigFacets/BillingFacet.sol/BillingFacet.json",
        ),
        (
            "ViewsFacet",
            "AccountConfigFacets/ViewsFacet.sol/ViewsFacet.json",
        ),
        (
            "WritesFacet",
            "AccountConfigFacets/WritesFacet.sol/WritesFacet.json",
        ),
    ] {
        let facet = deploy_facet_from_json(abis_folder, json_path, client.clone()).await?;
        println!("Prepared facet {name}");
        facet_cuts.extend(get_facet_cuts(&facet, existing_selectors, display));
    }

    let args = encode_constructor_args(&[
        DynSolValue::Address(owner),
        facet_cuts_to_dyn(&facet_cuts),
        DynSolValue::Address(*diamond_init.address()),
        DynSolValue::Bytes(init_calldata(&diamond_init)?.to_vec()),
    ]);

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
    let client = signer_provider(rpc_url, chain_id, secret)?;

    let data = build_diamond_update(client.clone(), abis_folder, diamond_address).await?;

    print!("Cutting diamond with init  {:?} ...", data.init_calldata);
    let calldata = diamond_cut_calldata(&client, diamond_address, &data).await?;
    let tx = legacy_tx_request()
        .with_to(diamond_address)
        .with_input(calldata);
    let receipt = client.send_transaction(tx).await?.get_receipt().await?;
    if !receipt.status() {
        return Err("diamondCut transaction reverted".into());
    }
    println!("Diamond contract updated!");

    let facet_addresses = facet_addresses(&client, diamond_address).await?;
    let mut new_selectors: Vec<FixedBytes<4>> = Vec::new();
    for facet_address in &facet_addresses {
        let selectors = facet_function_selectors(&client, diamond_address, *facet_address).await?;
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
pub async fn propose_update_diamond(
    rpc_url: &str,
    chain_id: u64,
    abis_folder: &str,
    secret: &str,
    diamond_address: Address,
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = signer_provider(rpc_url, chain_id, secret)?;

    let data = build_diamond_update(client.clone(), abis_folder, diamond_address).await?;
    let calldata = diamond_cut_calldata(&client, diamond_address, &data).await?;

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
        "data": format!("0x{}", alloy::hex::encode(&calldata)),
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

async fn diamond_cut_calldata(
    client: &SigningProvider,
    diamond_address: Address,
    data: &DiamondUpdateData,
) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
    let contract = diamond_cut_contract(client.clone(), diamond_address)?;
    let call = contract.function(
        "diamondCut",
        &[
            facet_cuts_to_dyn(&data.facet_cuts),
            DynSolValue::Address(data.init_address),
            DynSolValue::Bytes(data.init_calldata.to_vec()),
        ],
    )?;
    Ok(call.calldata().clone())
}

fn facet_cuts_to_dyn(facet_cuts: &[FacetCut]) -> DynSolValue {
    DynSolValue::Array(
        facet_cuts
            .iter()
            .map(|cut| {
                DynSolValue::Tuple(vec![
                    DynSolValue::Address(cut.facetAddress),
                    DynSolValue::Uint(U256::from(cut.action), 8),
                    DynSolValue::Array(
                        cut.functionSelectors
                            .iter()
                            .map(|selector| DynSolValue::FixedBytes(selector_to_word(selector), 4))
                            .collect(),
                    ),
                ])
            })
            .collect(),
    )
}

fn dyn_value_to_selector(
    value: &DynSolValue,
) -> Result<FixedBytes<4>, Box<dyn std::error::Error + Send + Sync>> {
    match value {
        DynSolValue::FixedBytes(word, 4) => Ok(FixedBytes::<4>::from_slice(&word[..4])),
        _ => Err("unexpected selector value".into()),
    }
}

fn selector_to_word(selector: &FixedBytes<4>) -> B256 {
    let mut word = [0u8; 32];
    word[..4].copy_from_slice(selector.as_slice());
    B256::from(word)
}

fn init_calldata(
    diamond_init: &DeployedContract,
) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
    let mut init_functions = diamond_init
        .abi()
        .functions()
        .filter(|function| function.name == "init");
    let init = init_functions
        .next()
        .ok_or("DiamondInit artifact missing init function")?;
    if init_functions.next().is_some() {
        return Err("DiamondInit artifact has multiple init overloads".into());
    }
    if !init.inputs.is_empty() {
        return Err("DiamondInit init function is expected to have no inputs".into());
    }
    Ok(Bytes::copy_from_slice(init.selector().as_slice()))
}

fn action_to_string(action: u8) -> String {
    match action {
        0 => String::from("Add"),
        1 => String::from("Replace"),
        2 => String::from("Remove"),
        _ => String::from("Unknown"),
    }
}
