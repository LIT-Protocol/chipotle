//! Deploy contracts from a folder of ABI JSON artifacts to a chain.
//!
//! Usage: deploy --action=<action> --network=<network> --abifolder=<abifolder> [--secret=<secret>] [--address=<address>]
//!
//! action:    deploy, update, or propose-update
//! network:   anvil, yellowstone, base-sepolia, or base
//! abifolder: folder containing contract artifact JSON files (abi + bytecode)
//! secret:    optional; deployer private key (hex). If blank or omitted, uses default Anvil dev secret.
//! address:   required for update and propose-update actions; the diamond contract address.
//! output:    optional; path for the proposal JSON file (propose-update only). Defaults to diamond_cut_proposal.json.

use ethers::types::Address;
use lit_contracts_minimal_generator::args::{get_network_and_chain_id, parse_named_args};
use lit_contracts_minimal_generator::deployer::diamond::{
    deploy_diamond, propose_update_diamond, update_diamond,
};
use std::env;

/// Default Anvil account #0 private key (well-known for local dev)
const DEFAULT_SECRET: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let named = parse_named_args(&args);

    let bin = args.first().map(|s| s.as_str()).unwrap_or("deploy");
    let usage = || {
        eprintln!(
            "Usage: {} --action=<action> --network=<network> --abifolder=<abifolder> [--secret=<secret>] [--address=<address>] [--output=<path>]",
            bin
        );
        eprintln!("  --action    deploy, update, or propose-update");
        eprintln!("  --network   anvil, yellowstone, base-sepolia, or base");
        eprintln!("  --abifolder folder containing contract artifact JSON files (abi + bytecode)");
        eprintln!(
            "  --secret    optional deployer private key (hex). If blank or omitted, uses Anvil account #0."
        );
        eprintln!(
            "  --address   diamond contract address (required for update and propose-update)"
        );
        eprintln!(
            "  --output    proposal JSON output path (propose-update only, default: diamond_cut_proposal.json)"
        );
        std::process::exit(1);
    };

    let action = match named.get("action") {
        Some(a) => a.to_lowercase(),
        None => {
            eprintln!("Missing required arg: --action");
            usage();
            std::process::exit(1);
        }
    };
    if action != "deploy" && action != "update" && action != "propose-update" {
        eprintln!("--action must be deploy, update, or propose-update");
        usage();
        std::process::exit(1);
    }

    let network = match named.get("network") {
        Some(n) => n.to_lowercase(),
        None => {
            eprintln!("Missing required arg: --network");
            usage();
            std::process::exit(1);
        }
    };

    let (rpc_url, chain_id) = get_network_and_chain_id(network.as_str());

    let rpc_url = match named.get("rpc-url") {
        Some(r) => r.as_str(),
        None => rpc_url,
    };

    let abis_folder = match named.get("abifolder") {
        Some(f) => f.trim_end_matches('/').to_string(),
        None => {
            eprintln!("Missing required arg: --abifolder");
            usage();
            std::process::exit(1);
        }
    };

    let secret_owned = named
        .get("secret")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let secret: &str = secret_owned.as_deref().unwrap_or(DEFAULT_SECRET);

    if action == "deploy" {
        println!(
            "Deploying contracts from folder {} on network {} with RPC URL {}",
            abis_folder, network, rpc_url
        );

        deploy_diamond(rpc_url, chain_id, &abis_folder, secret)
            .await
            .expect("Failed to deploy diamond");
    }
    if action == "update" {
        println!(
            "Updating diamond facets from folder {} on network {} with RPC URL {}",
            abis_folder, network, rpc_url
        );

        let diamond_address = parse_diamond_address(&named, usage);

        update_diamond(rpc_url, chain_id, &abis_folder, secret, diamond_address)
            .await
            .expect("Failed to update diamond");
    }
    if action == "propose-update" {
        println!(
            "Proposing diamond update from folder {} on network {} with RPC URL {}",
            abis_folder, network, rpc_url
        );

        let diamond_address = parse_diamond_address(&named, usage);
        let output = named
            .get("output")
            .cloned()
            .unwrap_or_else(|| "diamond_cut_proposal.json".to_string());

        propose_update_diamond(
            rpc_url,
            chain_id,
            &abis_folder,
            secret,
            diamond_address,
            &output,
        )
        .await
        .expect("Failed to propose diamond update");
    }
    Ok(())
}

fn parse_diamond_address(
    named: &std::collections::HashMap<String, String>,
    usage: impl Fn(),
) -> Address {
    let diamond_address_str = match named.get("address") {
        Some(a) => a.clone(),
        None => {
            eprintln!("Missing required arg: --address (required for update/propose-update)");
            usage();
            std::process::exit(1);
        }
    };
    let diamond_address_bytes =
        hex::decode(diamond_address_str.trim_start_matches("0x")).expect("Invalid --address hex");
    Address::from_slice(&diamond_address_bytes)
}
