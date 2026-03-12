use super::deploy::{deploy_artifact, deploy_contract};
use crate::diamond::c_diamond_cut_facet::FacetCut;
use crate::diamond::c_diamond_cut_facet::{
    DIAMONDCUTFACET_ABI, DIAMONDCUTFACET_BYTECODE, DiamondCutFacet,
};
use crate::diamond::c_diamond_loupe_facet::{
    DIAMONDLOUPEFACET_ABI, DIAMONDLOUPEFACET_BYTECODE, DiamondLoupeFacet,
};
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
    existing_selectors: &Vec<[u8; 4]>,
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
    let existing_selectors = &Vec::new();
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

    let diamond_loop_facet = DiamondLoupeFacet::new(diamond_address, client.clone());
    let facet_addresses = diamond_loop_facet.facet_addresses().call().await?;

    let diamond_cut_facet = DiamondCutFacet::new(diamond_address, client.clone());
    let mut existing_selectors: Vec<[u8; 4]> = Vec::new();
    for facet_address in &facet_addresses {
        let selectors = diamond_loop_facet
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

    let api_config = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/APIConfigFacet.sol/APIConfigFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&api_config, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);

    let billing_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/BillingFacet.sol/BillingFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&billing_facet, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);

    let views_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/ViewsFacet.sol/ViewsFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&views_facet, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);

    let writes_facet = deploy_facet_from_json(
        abis_folder,
        "AccountConfigFacets/WritesFacet.sol/WritesFacet.json",
        client.clone(),
    )
    .await?;
    let facet_cuts_for_contract = get_facet_cuts(&writes_facet, &existing_selectors, true);
    facet_cuts.extend(facet_cuts_for_contract);

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

    print!("Cutting diamond with init  {:?} ...", init);
    let tx = diamond_cut_facet.diamond_cut(facet_cuts, diamond_init.address(), Bytes::from(init));
    let pending_tx = tx.send().await?;
    let _receipt = pending_tx.await?;
    println!("Diamond contract updated!");
    let facet_addresses = diamond_loop_facet.facet_addresses().call().await?;
    let mut new_selectors: Vec<[u8; 4]> = Vec::new();
    for facet_address in &facet_addresses {
        let selectors = diamond_loop_facet
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

fn action_to_string(action: u8) -> String {
    match action {
        0 => String::from("Add"),
        1 => String::from("Replace"),
        2 => String::from("Remove"),
        _ => String::from("Unknown"),
    }
}
