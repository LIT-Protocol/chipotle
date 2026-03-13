const ANVIL_RPC: &str = "http://127.0.0.1:8545";
const ANVIL_CHAIN_ID: u64 = 31337;
const YELLOWSTONE_RPC: &str = "https://yellowstone-rpc.litprotocol.com";
const YELLOWSTONE_CHAIN_ID: u64 = 175188;
const BASE_SEPOLIA_RPC: &str = "https://sepolia.base.org";
const BASE_SEPOLIA_CHAIN_ID: u64 = 84532;
const BASE_RPC: &str = "https://mainnet.base.org";
const BASE_CHAIN_ID: u64 = 8453;

pub fn parse_named_args(args: &[String]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(kv) = arg.strip_prefix("--") {
            if let Some((k, v)) = kv.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            } else if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                map.insert(kv.to_string(), args[i + 1].clone());
                i += 1;
            }
        }
        i += 1;
    }
    map
}

pub fn get_network_and_chain_id(network: &str) -> (&'static str, u64) {
    match network {
        "anvil" => (ANVIL_RPC, ANVIL_CHAIN_ID),
        "yellowstone" => (YELLOWSTONE_RPC, YELLOWSTONE_CHAIN_ID),
        "base-sepolia" => (BASE_SEPOLIA_RPC, BASE_SEPOLIA_CHAIN_ID),
        "base" => (BASE_RPC, BASE_CHAIN_ID),
        _ => {
            eprintln!("--network must be anvil, yellowstone, base-sepolia, or base");
            std::process::exit(1);
        }
    }
}
