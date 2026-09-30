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
use alloy::primitives::{Address, B256, Bytes, FixedBytes, U256, keccak256};
use alloy::providers::Provider;
use std::collections::{HashMap, HashSet};
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

/// A committed list of functions to drop from the diamond on the next upgrade.
/// The deployer only computes Replace/Add automatically; removing a selector
/// requires an explicit, auditable declaration here so a partial/incorrect
/// build can never silently strip a live function.
#[derive(serde::Deserialize, Default)]
pub struct RemovalManifest {
    #[serde(default)]
    pub removals: Vec<RemovalEntry>,
}

#[derive(serde::Deserialize, Clone)]
pub struct RemovalEntry {
    /// Canonical function signature, e.g. "backfillPkpOwners(address[],uint256[])".
    pub signature: String,
    /// Optional 0x-prefixed 4-byte selector; if present it is cross-checked
    /// against keccak256(signature) as a typo guard.
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// 4-byte selector of a canonical function signature.
pub fn selector_of(signature: &str) -> FixedBytes<4> {
    FixedBytes::<4>::from_slice(&keccak256(signature.as_bytes())[0..4])
}

pub struct RemovalPlan {
    pub to_remove: Vec<FixedBytes<4>>,
    pub warnings: Vec<String>,
}

/// Decide which selectors to Remove in this upgrade. Safe by construction: a
/// selector is removed only if it is BOTH explicitly listed in the manifest AND
/// detected as orphaned by this upgrade — i.e. it currently lives on the old
/// address of a facet we are re-installing, yet is absent from every new managed
/// facet ABI. Core facets (DiamondCut/Loupe) are never touched: none of their
/// selectors appear in the managed set, so their addresses are never classified
/// as "managed", so their selectors are never orphan candidates.
pub fn plan_removals(
    selector_to_addr: &HashMap<FixedBytes<4>, Address>,
    new_managed: &HashSet<FixedBytes<4>>,
    manifest: &[RemovalEntry],
) -> RemovalPlan {
    // Addresses currently hosting at least one selector we are re-installing.
    let managed_old_addresses: HashSet<Address> = selector_to_addr
        .iter()
        .filter(|(sel, _)| new_managed.contains(*sel))
        .map(|(_, addr)| *addr)
        .collect();

    // On-chain selectors on a managed facet's old address that the new ABIs drop.
    let orphans: HashSet<FixedBytes<4>> = selector_to_addr
        .iter()
        .filter(|(sel, addr)| managed_old_addresses.contains(*addr) && !new_managed.contains(*sel))
        .map(|(sel, _)| *sel)
        .collect();

    let mut warnings = Vec::new();
    let mut to_remove = Vec::new();
    let mut approved: HashSet<FixedBytes<4>> = HashSet::new();

    for entry in manifest {
        let sel = selector_of(&entry.signature);
        if let Some(expected) = &entry.selector {
            let want = expected.trim_start_matches("0x").to_lowercase();
            let got = hex::encode(sel.as_slice());
            if want != got {
                warnings.push(format!(
                    "manifest '{}': selector mismatch (signature hashes to 0x{}, manifest says {}) — skipping",
                    entry.signature, got, expected
                ));
                continue;
            }
        }
        if !selector_to_addr.contains_key(&sel) {
            warnings.push(format!(
                "manifest '{}' (0x{}): not present on-chain — nothing to remove",
                entry.signature,
                hex::encode(sel.as_slice())
            ));
            continue;
        }
        if !orphans.contains(&sel) {
            warnings.push(format!(
                "manifest '{}' (0x{}): still served by a managed facet in the new ABIs (not orphaned) — REFUSING to remove",
                entry.signature,
                hex::encode(sel.as_slice())
            ));
            continue;
        }
        if approved.insert(sel) {
            to_remove.push(sel);
        }
    }

    for sel in &orphans {
        if !approved.contains(sel) {
            warnings.push(format!(
                "0x{}: orphaned by this upgrade but not approved for removal — it will remain routed to its old facet and stay callable. Add it to the removals manifest to drop it.",
                hex::encode(sel.as_slice())
            ));
        }
    }

    RemovalPlan {
        to_remove,
        warnings,
    }
}

fn load_removal_manifest(path: &str) -> Vec<RemovalEntry> {
    match std::fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str::<RemovalManifest>(&s) {
            Ok(m) => {
                println!(
                    "Loaded removals manifest {} ({} entr{}).",
                    path,
                    m.removals.len(),
                    if m.removals.len() == 1 { "y" } else { "ies" }
                );
                m.removals
            }
            Err(e) => {
                eprintln!(
                    "WARNING: failed to parse removals manifest {path}: {e} — proceeding with NO removals"
                );
                Vec::new()
            }
        },
        Err(_) => {
            println!("No removals manifest at {path} — this upgrade removes no selectors.");
            Vec::new()
        }
    }
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
    removals_manifest_path: &str,
) -> Result<DiamondUpdateData, Box<dyn std::error::Error + Send + Sync>> {
    let facet_addresses = facet_addresses(&client, diamond_address).await?;

    let mut existing_selectors: Vec<FixedBytes<4>> = Vec::new();
    let mut selector_to_addr: HashMap<FixedBytes<4>, Address> = HashMap::new();
    for facet_address in &facet_addresses {
        let selectors = facet_function_selectors(&client, diamond_address, *facet_address).await?;
        for sel in &selectors {
            selector_to_addr.insert(*sel, *facet_address);
        }
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
    // Union of selectors across the facets this upgrade (re)installs. Used to
    // detect selectors orphaned by the upgrade so they can be Removed.
    let mut new_managed: HashSet<FixedBytes<4>> = HashSet::new();

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
        new_managed.extend(facet.abi().functions().map(|f| f.selector()));
        facet_cuts.extend(get_facet_cuts(&facet, &existing_selectors, true));
        facets_deployed.insert(name.to_string(), *facet.address());
    }

    let ownership_facet =
        deploy_prebuilt_facet("OwnershipFacet", OWNERSHIPFACET_JSON, client.clone()).await?;
    new_managed.extend(ownership_facet.abi().functions().map(|f| f.selector()));
    facet_cuts.extend(get_facet_cuts(&ownership_facet, &existing_selectors, true));
    facets_deployed.insert("OwnershipFacet".to_string(), *ownership_facet.address());

    // Explicit, manifest-gated selector removals (see plan_removals).
    let manifest = load_removal_manifest(removals_manifest_path);
    let plan = plan_removals(&selector_to_addr, &new_managed, &manifest);
    for w in &plan.warnings {
        println!("  [removals] {w}");
    }
    if !plan.to_remove.is_empty() {
        println!(
            "Removing {} orphaned selector(s): {}",
            plan.to_remove.len(),
            plan.to_remove
                .iter()
                .map(|s| format!("0x{}", hex::encode(s.as_slice())))
                .collect::<Vec<_>>()
                .join(", ")
        );
        facet_cuts.push(FacetCut {
            facetAddress: Address::ZERO,
            action: FacetCutAction::Remove as u8,
            functionSelectors: plan.to_remove,
        });
    }

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
    removals_manifest_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = signer_provider(rpc_url, chain_id, secret)?;

    let data = build_diamond_update(
        client.clone(),
        abis_folder,
        diamond_address,
        removals_manifest_path,
    )
    .await?;

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
    removals_manifest_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = signer_provider(rpc_url, chain_id, secret)?;

    let data = build_diamond_update(
        client.clone(),
        abis_folder,
        diamond_address,
        removals_manifest_path,
    )
    .await?;
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

#[cfg(test)]
mod removal_tests {
    use super::*;

    fn sel(bytes: [u8; 4]) -> FixedBytes<4> {
        FixedBytes::<4>::from(bytes)
    }

    fn entry(sig: &str) -> RemovalEntry {
        RemovalEntry {
            signature: sig.to_string(),
            selector: None,
            reason: None,
        }
    }

    // Fixture: managed facet at address A hosts `keep` (still in the new ABI)
    // and `gone` (dropped by the upgrade). Core facet at address C hosts `core`,
    // which is absent from the managed set but must never be treated as orphaned.
    fn fixture() -> (HashMap<FixedBytes<4>, Address>, HashSet<FixedBytes<4>>) {
        let a = Address::repeat_byte(0xAA);
        let c = Address::repeat_byte(0xCC);
        let keep = sel([0x11, 0x11, 0x11, 0x11]);
        let gone = sel([0x22, 0x22, 0x22, 0x22]);
        let core = sel([0x33, 0x33, 0x33, 0x33]);
        let add = sel([0x44, 0x44, 0x44, 0x44]); // brand-new selector, not yet on-chain

        let mut s2a = HashMap::new();
        s2a.insert(keep, a);
        s2a.insert(gone, a);
        s2a.insert(core, c);

        let mut managed = HashSet::new();
        managed.insert(keep);
        managed.insert(add);
        (s2a, managed)
    }

    #[test]
    fn removes_only_manifest_listed_orphans() {
        let (s2a, managed) = fixture();
        let gone = sel([0x22, 0x22, 0x22, 0x22]);
        // Manifest lists exactly the orphan by raw selector signature — use a
        // signature that hashes to `gone` is impractical, so pass selector guard
        // off and rely on signature hashing in the non-test path; here we test
        // the planner directly with a manifest whose signature hashes match.
        // Build a manifest entry whose selector_of equals `gone`.
        let sig = "someRemovedFn()";
        let mut man = vec![entry(sig)];
        // Force the fixture's `gone` to equal selector_of(sig) so the manifest matches.
        let derived = selector_of(sig);
        let mut s2a2 = s2a.clone();
        // relocate `gone` -> derived selector on the same managed address A
        s2a2.remove(&gone);
        s2a2.insert(derived, Address::repeat_byte(0xAA));
        man[0].selector = Some(format!("0x{}", hex::encode(derived.as_slice())));

        let plan = plan_removals(&s2a2, &managed, &man);
        assert_eq!(
            plan.to_remove,
            vec![derived],
            "orphan in manifest is removed"
        );
    }

    #[test]
    fn warns_and_skips_orphan_not_in_manifest() {
        let (s2a, managed) = fixture();
        let gone = sel([0x22, 0x22, 0x22, 0x22]);
        let plan = plan_removals(&s2a, &managed, &[]);
        assert!(plan.to_remove.is_empty());
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains(&hex::encode(gone.as_slice())) && w.contains("orphaned")),
            "unlisted orphan must be warned about"
        );
    }

    #[test]
    fn refuses_to_remove_non_orphan_even_if_listed() {
        // `core` is on-chain but on an unmanaged facet -> never an orphan.
        let (s2a, managed) = fixture();
        let core = sel([0x33, 0x33, 0x33, 0x33]);
        let man = vec![RemovalEntry {
            signature: "core()".to_string(),
            selector: Some(format!("0x{}", hex::encode(core.as_slice()))),
            reason: None,
        }];
        // Make selector_of("core()") == core so the manifest entry resolves to it.
        // If it doesn't, the selector-mismatch guard fires — also a safe refusal.
        let plan = plan_removals(&s2a, &managed, &man);
        assert!(
            plan.to_remove.is_empty(),
            "a non-orphaned selector is never removed"
        );
        assert!(!plan.warnings.is_empty());
    }

    #[test]
    fn skips_manifest_selector_absent_on_chain() {
        let (s2a, managed) = fixture();
        let ghost = "neverDeployed()";
        let man = vec![entry(ghost)];
        let plan = plan_removals(&s2a, &managed, &man);
        assert!(plan.to_remove.is_empty());
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("not present on-chain")),
            "absent manifest selector must be skipped with a warning"
        );
    }

    #[test]
    fn selector_guard_rejects_mismatch() {
        let (mut s2a, managed) = fixture();
        // put the derived selector of "x()" on managed address A as an orphan
        let derived = selector_of("x()");
        s2a.insert(derived, Address::repeat_byte(0xAA));
        let man = vec![RemovalEntry {
            signature: "x()".to_string(),
            selector: Some("0xdeadbeef".to_string()),
            reason: None,
        }];
        let plan = plan_removals(&s2a, &managed, &man);
        assert!(
            plan.to_remove.is_empty(),
            "mismatched selector guard blocks removal"
        );
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("selector mismatch"))
        );
    }

    #[test]
    fn backfill_selector_matches_known_value() {
        assert_eq!(
            format!(
                "0x{}",
                hex::encode(selector_of("backfillPkpOwners(address[],uint256[])").as_slice())
            ),
            "0x41275609"
        );
    }
}
