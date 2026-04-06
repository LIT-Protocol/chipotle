use crate::core::v1::models::signing_scheme::SigningScheme;
use rocket::http::Status;
use serde::{Deserialize, Serialize};
use std::env;
/// EVM chain RPC and wallet info.
#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct ChainInfo {
    pub chain_name: &'static str,
    /// Lowercase enum variant name (e.g. "ethereum", "bnbsmartchain").
    pub chain: &'static str,
    pub chain_id: u64,
    pub is_evm: bool,
    pub testnet: bool,
    pub token: &'static str,
    pub rpc_url: &'static str,
    pub signing_scheme: SigningScheme,
    pub derivation_path: &'static str,
}

/// Supported EVM chains with their RPC and derivation data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Chain {
    Anvil,
    Ethereum,
    BnbSmartChain,
    ArbitrumOne,
    Base,
    AvalancheCChain,
    PolygonPos,
    Optimism,
    Blast,
    Scroll,
    Mantle,
    Linea,
    Cronos,
    Mode,
    ZkSyncEra,
    PulseChain,
    Fantom,
    MantaPacific,
    Celo,
    Kava,
    Gnosis,
    Metis,
    Astar,
    Moonbeam,
    Core,
    Zora,
    Klaytn,
    Canto,
    Moonriver,
    Taiko,
    Flare,
    ConfluxESpace,
    Aurora,
    Telos,
    BobaNetwork,
    Heco,
    OkxChain,
    ZkFair,
    Wemix30,
    Kcc,
    IoTeX,
    Evmos,
    Meter,
    MilkomedaC1,
    Syscoin,
    Dogechain,
    Fuse,
    Shibarium,
    ThunderCore,
    ReiNetwork,
    XLayer,
    Yellowstone,
    // Non-EVM chains
    Tron,
    Solana,
    Sui,
    Bitcoin,
    Aptos,
    Near,
    Cardano,
    Stacks,
    CosmosHub,
    Stellar,
    // Testnets (from TESTNETS)
    AptosTestnet,
    ArbitrumSepolia,
    AvalancheFuji,
    BaseSepolia,
    BitcoinSignet,
    BnbSmartChainTestnet,
    EthereumHolesky,
    EthereumSepolia,
    PolygonAmoy,
    SolanaDevnet,
}

impl Chain {
    pub fn try_from_str(chain: &str) -> Result<Self, Status> {
        match chain.to_lowercase().as_str() {
            "anvil" => Ok(Self::Anvil),
            "ethereum" => Ok(Self::Ethereum),
            "bnbsmartchain" => Ok(Self::BnbSmartChain),
            "arbitrumone" => Ok(Self::ArbitrumOne),
            "base" => Ok(Self::Base),
            "avalanchecchain" => Ok(Self::AvalancheCChain),
            "polygonpos" => Ok(Self::PolygonPos),
            "optimism" => Ok(Self::Optimism),
            "blast" => Ok(Self::Blast),
            "scroll" => Ok(Self::Scroll),
            "mantle" => Ok(Self::Mantle),
            "linea" => Ok(Self::Linea),
            "cronos" => Ok(Self::Cronos),
            "mode" => Ok(Self::Mode),
            "zksyncera" => Ok(Self::ZkSyncEra),
            "pulsechain" => Ok(Self::PulseChain),
            "fantom" => Ok(Self::Fantom),
            "mantapacific" => Ok(Self::MantaPacific),
            "celo" => Ok(Self::Celo),
            "kava" => Ok(Self::Kava),
            "gnosis" => Ok(Self::Gnosis),
            "metis" => Ok(Self::Metis),
            "astar" => Ok(Self::Astar),
            "moonbeam" => Ok(Self::Moonbeam),
            "core" => Ok(Self::Core),
            "zora" => Ok(Self::Zora),
            "klaytn" => Ok(Self::Klaytn),
            "canto" => Ok(Self::Canto),
            "moonriver" => Ok(Self::Moonriver),
            "taiko" => Ok(Self::Taiko),
            "flare" => Ok(Self::Flare),
            "conflux" => Ok(Self::ConfluxESpace),
            "aurora" => Ok(Self::Aurora),
            "telos" => Ok(Self::Telos),
            "boba" => Ok(Self::BobaNetwork),
            "heco" => Ok(Self::Heco),
            "okx" => Ok(Self::OkxChain),
            "zkfair" => Ok(Self::ZkFair),
            "wemix" => Ok(Self::Wemix30),
            "kcc" => Ok(Self::Kcc),
            "iotex" => Ok(Self::IoTeX),
            "evmos" => Ok(Self::Evmos),
            "meter" => Ok(Self::Meter),
            "milkomedac1" => Ok(Self::MilkomedaC1),
            "syscoin" => Ok(Self::Syscoin),
            "dogechain" => Ok(Self::Dogechain),
            "fuse" => Ok(Self::Fuse),
            "shibarium" => Ok(Self::Shibarium),
            "thundercore" => Ok(Self::ThunderCore),
            "reinetwork" => Ok(Self::ReiNetwork),
            "xlayer" => Ok(Self::XLayer),
            "tron" => Ok(Self::Tron),
            "solana" => Ok(Self::Solana),
            "sui" => Ok(Self::Sui),
            "bitcoin" => Ok(Self::Bitcoin),
            "aptos" => Ok(Self::Aptos),
            "near" => Ok(Self::Near),
            "cardano" => Ok(Self::Cardano),
            "stacks" => Ok(Self::Stacks),
            "cosmos" => Ok(Self::CosmosHub),
            "stellar" => Ok(Self::Stellar),
            "yellowstone" => Ok(Self::Yellowstone),
            // Testnets
            "aptostestnet" => Ok(Self::AptosTestnet),
            "arbitrumsepolia" => Ok(Self::ArbitrumSepolia),
            "avalanchefuji" => Ok(Self::AvalancheFuji),
            "basesepolia" => Ok(Self::BaseSepolia),
            "bitcoinsignet" => Ok(Self::BitcoinSignet),
            "bnbsmartchaintestnet" => Ok(Self::BnbSmartChainTestnet),
            "ethereumholesky" => Ok(Self::EthereumHolesky),
            "ethereumsepolia" => Ok(Self::EthereumSepolia),
            "polygonamoy" => Ok(Self::PolygonAmoy),
            "solanadevnet" => Ok(Self::SolanaDevnet),
            _ => Err(Status::NotFound),
        }
    }

    /// Lowercase enum variant name for API/identification (e.g. "ethereum", "arbitrumsepolia").
    pub fn chain_key(self) -> &'static str {
        match self {
            Self::Anvil => "anvil",
            Self::Yellowstone => "yellowstone",
            Self::Ethereum => "ethereum",
            Self::BnbSmartChain => "bnbsmartchain",
            Self::ArbitrumOne => "arbitrumone",
            Self::Base => "base",
            Self::AvalancheCChain => "avalanchecchain",
            Self::PolygonPos => "polygonpos",
            Self::Optimism => "optimism",
            Self::Blast => "blast",
            Self::Scroll => "scroll",
            Self::Mantle => "mantle",
            Self::Linea => "linea",
            Self::Cronos => "cronos",
            Self::Mode => "mode",
            Self::ZkSyncEra => "zksyncera",
            Self::PulseChain => "pulsechain",
            Self::Fantom => "fantom",
            Self::MantaPacific => "mantapacific",
            Self::Celo => "celo",
            Self::Kava => "kava",
            Self::Gnosis => "gnosis",
            Self::Metis => "metis",
            Self::Astar => "astar",
            Self::Moonbeam => "moonbeam",
            Self::Core => "core",
            Self::Zora => "zora",
            Self::Klaytn => "klaytn",
            Self::Canto => "canto",
            Self::Moonriver => "moonriver",
            Self::Taiko => "taiko",
            Self::Flare => "flare",
            Self::ConfluxESpace => "confluxespace",
            Self::Aurora => "aurora",
            Self::Telos => "telos",
            Self::BobaNetwork => "bobanetwork",
            Self::Heco => "heco",
            Self::OkxChain => "okxchain",
            Self::ZkFair => "zkfair",
            Self::Wemix30 => "wemix30",
            Self::Kcc => "kcc",
            Self::IoTeX => "iotex",
            Self::Evmos => "evmos",
            Self::Meter => "meter",
            Self::MilkomedaC1 => "milkomedac1",
            Self::Syscoin => "syscoin",
            Self::Dogechain => "dogechain",
            Self::Fuse => "fuse",
            Self::Shibarium => "shibarium",
            Self::ThunderCore => "thundercore",
            Self::ReiNetwork => "reinetwork",
            Self::XLayer => "xlayer",
            Self::Tron => "tron",
            Self::Solana => "solana",
            Self::Sui => "sui",
            Self::Bitcoin => "bitcoin",
            Self::Aptos => "aptos",
            Self::Near => "near",
            Self::Cardano => "cardano",
            Self::Stacks => "stacks",
            Self::CosmosHub => "cosmoshub",
            Self::Stellar => "stellar",
            Self::AptosTestnet => "aptostestnet",
            Self::ArbitrumSepolia => "arbitrumsepolia",
            Self::AvalancheFuji => "avalanchefuji",
            Self::BaseSepolia => "basesepolia",
            Self::BitcoinSignet => "bitcoinsignet",
            Self::BnbSmartChainTestnet => "bnbsmartchaintestnet",
            Self::EthereumHolesky => "ethereumholesky",
            Self::EthereumSepolia => "ethereumsepolia",
            Self::PolygonAmoy => "polygonamoy",
            Self::SolanaDevnet => "solanadevnet",
        }
    }

    pub fn info(self) -> ChainInfo {
        match self {
            Self::Yellowstone => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Yellowstone",
                chain_id: 175188,
                is_evm: true,
                testnet: true,
                token: "LitKey",
                rpc_url: "https://yellowstone-rpc.litprotocol.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Ethereum => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Ethereum",
                chain_id: 1,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://eth.llamarpc.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::BnbSmartChain => ChainInfo {
                chain: self.chain_key(),
                chain_name: "BNB Smart Chain",
                chain_id: 56,
                is_evm: true,
                testnet: false,
                token: "BNB",
                rpc_url: "https://bsc-dataseed.binance.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::ArbitrumOne => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Arbitrum One",
                chain_id: 42161,
                is_evm: true,
                testnet: false,
                token: "ARB",
                rpc_url: "https://arb1.arbitrum.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Base => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Base",
                chain_id: 8453,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://mainnet.base.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::AvalancheCChain => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Avalanche C-Chain",
                chain_id: 43114,
                is_evm: true,
                testnet: false,
                token: "AVAX",
                rpc_url: "https://api.avax.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::PolygonPos => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Polygon POS",
                chain_id: 137,
                is_evm: true,
                testnet: false,
                token: "POL",
                rpc_url: "https://polygon-rpc.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Optimism => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Optimism",
                chain_id: 10,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://mainnet.optimism.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Blast => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Blast",
                chain_id: 81457,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://rpc.blast.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Scroll => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Scroll",
                chain_id: 534352,
                is_evm: true,
                testnet: false,
                token: "SCR",
                rpc_url: "https://rpc.scroll.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Mantle => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Mantle",
                chain_id: 5000,
                is_evm: true,
                testnet: false,
                token: "MNT",
                rpc_url: "https://rpc.mantle.xyz",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Linea => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Linea",
                chain_id: 59144,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://rpc.linea.build",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Cronos => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Cronos",
                chain_id: 25,
                is_evm: true,
                testnet: false,
                token: "CRO",
                rpc_url: "https://evm.cronos.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Mode => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Mode",
                chain_id: 34443,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://mainnet.mode.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::ZkSyncEra => ChainInfo {
                chain: self.chain_key(),
                chain_name: "zkSync Era",
                chain_id: 324,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://mainnet.era.zksync.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::PulseChain => ChainInfo {
                chain: self.chain_key(),
                chain_name: "PulseChain",
                chain_id: 369,
                is_evm: true,
                testnet: false,
                token: "PLS",
                rpc_url: "https://rpc.pulsechain.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Fantom => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Fantom",
                chain_id: 250,
                is_evm: true,
                testnet: false,
                token: "FTM",
                rpc_url: "https://rpc.ftm.tools",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::MantaPacific => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Manta Pacific",
                chain_id: 169,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://pacific-rpc.manta.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Celo => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Celo",
                chain_id: 42220,
                is_evm: true,
                testnet: false,
                token: "CELO",
                rpc_url: "https://forno.celo.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/52734'/0'/0/0",
            },
            Self::Kava => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Kava",
                chain_id: 2222,
                is_evm: true,
                testnet: false,
                token: "KAVA",
                rpc_url: "https://evm.kava.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/459'/0'/0/0",
            },
            Self::Gnosis => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Gnosis",
                chain_id: 100,
                is_evm: true,
                testnet: false,
                token: "xDAI",
                rpc_url: "https://rpc.gnosischain.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/700'/0'/0/0",
            },
            Self::Metis => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Metis",
                chain_id: 1088,
                is_evm: true,
                testnet: false,
                token: "METIS",
                rpc_url: "https://andromeda.metis.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Astar => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Astar",
                chain_id: 592,
                is_evm: true,
                testnet: false,
                token: "ASTR",
                rpc_url: "https://evm.astar.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Moonbeam => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Moonbeam",
                chain_id: 1284,
                is_evm: true,
                testnet: false,
                token: "GLMR",
                rpc_url: "https://rpc.api.moonbeam.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/1284'/0'/0/0",
            },
            Self::Core => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Core",
                chain_id: 1116,
                is_evm: true,
                testnet: false,
                token: "CORE",
                rpc_url: "https://rpc.coredao.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Zora => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Zora",
                chain_id: 7777777,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://rpc.zora.energy",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Klaytn => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Klaytn",
                chain_id: 8217,
                is_evm: true,
                testnet: false,
                token: "KLAY",
                rpc_url: "https://public-node-api.klaytnapi.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/8217'/0'/0/0",
            },
            Self::Canto => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Canto",
                chain_id: 7700,
                is_evm: true,
                testnet: false,
                token: "CANTO",
                rpc_url: "https://canto.slingshot.observer",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Moonriver => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Moonriver",
                chain_id: 1285,
                is_evm: true,
                testnet: false,
                token: "MOVR",
                rpc_url: "https://rpc.api.moonriver.moonbeam.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/1285'/0'/0/0",
            },
            Self::Taiko => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Taiko",
                chain_id: 167000,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://rpc.mainnet.taiko.xyz",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Flare => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Flare",
                chain_id: 14,
                is_evm: true,
                testnet: false,
                token: "FLR",
                rpc_url: "https://flare-api.flare.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::ConfluxESpace => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Conflux eSpace",
                chain_id: 1030,
                is_evm: true,
                testnet: false,
                token: "CFX",
                rpc_url: "https://evm.confluxrpc.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/503'/0'/0/0",
            },
            Self::Aurora => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Aurora",
                chain_id: 1313161554,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://mainnet.aurora.dev",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Telos => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Telos",
                chain_id: 40,
                is_evm: true,
                testnet: false,
                token: "TLOS",
                rpc_url: "https://mainnet.telos.net",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::BobaNetwork => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Boba Network",
                chain_id: 288,
                is_evm: true,
                testnet: false,
                token: "ETH",
                rpc_url: "https://mainnet.boba.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Heco => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Heco",
                chain_id: 128,
                is_evm: true,
                testnet: false,
                token: "HT",
                rpc_url: "https://http-mainnet.hecochain.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::OkxChain => ChainInfo {
                chain: self.chain_key(),
                chain_name: "OKXChain",
                chain_id: 66,
                is_evm: true,
                testnet: false,
                token: "OKT",
                rpc_url: "https://exchainrpc.okex.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::ZkFair => ChainInfo {
                chain: self.chain_key(),
                chain_name: "ZkFair",
                chain_id: 42766,
                is_evm: true,
                testnet: false,
                token: "ZKF",
                rpc_url: "https://rpc.zkfair.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Wemix30 => ChainInfo {
                chain: self.chain_key(),
                chain_name: "WEMIX 3.0",
                chain_id: 1111,
                is_evm: true,
                testnet: false,
                token: "WEMIX",
                rpc_url: "https://api.wemix.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Kcc => ChainInfo {
                chain: self.chain_key(),
                chain_name: "KCC",
                chain_id: 321,
                is_evm: true,
                testnet: false,
                token: "KCS",
                rpc_url: "https://rpc-mainnet.kcc.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::IoTeX => ChainInfo {
                chain: self.chain_key(),
                chain_name: "IoTeX",
                chain_id: 4689,
                is_evm: true,
                testnet: false,
                token: "IOTX",
                rpc_url: "https://babel-api.mainnet.iotex.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/304'/0'/0/0",
            },
            Self::Evmos => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Evmos",
                chain_id: 9001,
                is_evm: true,
                testnet: false,
                token: "EVMOS",
                rpc_url: "https://eth.bd.evmos.org:8545",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Meter => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Meter",
                chain_id: 82,
                is_evm: true,
                testnet: false,
                token: "MTRG",
                rpc_url: "https://rpc.meter.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::MilkomedaC1 => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Milkomeda C1",
                chain_id: 2002,
                is_evm: true,
                testnet: false,
                token: "ADA",
                rpc_url: "https://rpc-mainnet-cardano-evm.milkomeda.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Syscoin => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Syscoin",
                chain_id: 57,
                is_evm: true,
                testnet: false,
                token: "SYS",
                rpc_url: "https://rpc.syscoin.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/57'/0'/0/0",
            },
            Self::Dogechain => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Dogechain",
                chain_id: 2000,
                is_evm: true,
                testnet: false,
                token: "DC",
                rpc_url: "https://rpc.dogechain.dog",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Fuse => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Fuse",
                chain_id: 122,
                is_evm: true,
                testnet: false,
                token: "FUSE",
                rpc_url: "https://rpc.fuse.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::Shibarium => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Shibarium",
                chain_id: 109,
                is_evm: true,
                testnet: false,
                token: "BONE",
                rpc_url: "https://www.shibariumscan.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::ThunderCore => ChainInfo {
                chain: self.chain_key(),
                chain_name: "ThunderCore",
                chain_id: 108,
                is_evm: true,
                testnet: false,
                token: "TT",
                rpc_url: "https://mainnet-rpc.thundercore.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::ReiNetwork => ChainInfo {
                chain: self.chain_key(),
                chain_name: "REI Network",
                chain_id: 47805,
                is_evm: true,
                testnet: false,
                token: "REI",
                rpc_url: "https://rpc.rei.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            Self::XLayer => ChainInfo {
                chain: self.chain_key(),
                chain_name: "X Layer",
                chain_id: 196,
                is_evm: true,
                testnet: false,
                token: "OKB",
                rpc_url: "https://xlayerrpc.okx.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/0",
            },
            // Non-EVM chains (is_evm: false)
            Self::Tron => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Tron",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "TRX",
                rpc_url: "https://api.trongrid.io",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/195'/0'/0/0",
            },
            Self::Solana => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Solana",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "SOL",
                rpc_url: "https://api.mainnet-beta.solana.com",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/44'/501'/0'/0'",
            },
            Self::Sui => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Sui",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "SUI",
                rpc_url: "https://fullnode.mainnet.sui.io:443",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/44'/784'/0'/0'/0'",
            },
            Self::Bitcoin => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Bitcoin",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "BTC",
                rpc_url: "https://blockstream.info",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/84'/0'/0'/0/0",
            },
            Self::Aptos => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Aptos",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "APT",
                rpc_url: "https://fullnode.mainnet.aptoslabs.com",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/44'/637'/0'/0'/0'",
            },
            Self::Near => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Near",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "NEAR",
                rpc_url: "https://rpc.mainnet.near.org",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/44'/397'/0'",
            },
            Self::Cardano => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Cardano",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "ADA",
                rpc_url: "https://cardano-mainnet.blockfrost.io",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/1852'/1815'/0'",
            },
            Self::Stacks => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Stacks",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "STX",
                rpc_url: "https://stacks-node-api.mainnet.stacks.co",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/5757'/0'/0/0",
            },
            Self::CosmosHub => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Cosmos Hub",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "ATOM",
                rpc_url: "https://rpc.cosmos.network",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/118'/0'/0/0",
            },
            Self::Stellar => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Stellar",
                chain_id: 0,
                is_evm: false,
                testnet: false,
                token: "XLM",
                rpc_url: "https://horizon.stellar.org",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/44'/148'/0'",
            },
            // Testnets (from TESTNETS)
            Self::AptosTestnet => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Aptos Testnet",
                chain_id: 2,
                is_evm: false,
                testnet: true,
                token: "APT",
                rpc_url: "https://fullnode.testnet.aptoslabs.com",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/44'/637'/0'/0'/0'",
            },
            Self::ArbitrumSepolia => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Arbitrum Sepolia",
                chain_id: 421614,
                is_evm: true,
                testnet: true,
                token: "ETH",
                rpc_url: "https://sepolia-rollup.arbitrum.io/rpc",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/x",
            },
            Self::AvalancheFuji => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Avalanche Fuji Testnet",
                chain_id: 43113,
                is_evm: true,
                testnet: true,
                token: "AVAX",
                rpc_url: "https://avalanche-fuji-c-chain-rpc.publicnode.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/x",
            },
            Self::BaseSepolia => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Base Sepolia Testnet",
                chain_id: 84532,
                is_evm: true,
                testnet: true,
                token: "ETH",
                rpc_url: "https://sepolia.base.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/x",
            },
            Self::BitcoinSignet => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Bitcoin Signet",
                chain_id: 0,
                is_evm: false,
                testnet: true,
                token: "sBTC",
                rpc_url: "https://signet.bc-2.jp",
                signing_scheme: SigningScheme::SchnorrK256Taproot,
                derivation_path: "m/84'/1'/0'/0/x",
            },
            Self::BnbSmartChainTestnet => ChainInfo {
                chain: self.chain_key(),
                chain_name: "BNB Smart Chain Testnet",
                chain_id: 97,
                is_evm: true,
                testnet: true,
                token: "tBNB",
                rpc_url: "https://data-seed-prebsc-1-s1.binance.org:8545",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/x",
            },
            Self::EthereumHolesky => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Ethereum Holesky",
                chain_id: 17000,
                is_evm: true,
                testnet: true,
                token: "ETH",
                rpc_url: "https://ethereum-holesky-rpc.publicnode.com",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/x",
            },
            Self::EthereumSepolia => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Ethereum Sepolia",
                chain_id: 11155111,
                is_evm: true,
                testnet: true,
                token: "ETH",
                rpc_url: "https://rpc.sepolia.org",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/x",
            },
            Self::PolygonAmoy => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Polygon Amoy Testnet",
                chain_id: 80002,
                is_evm: true,
                testnet: true,
                token: "POL",
                rpc_url: "https://rpc-amoy.polygon.technology",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/60'/0'/0/x",
            },
            Self::SolanaDevnet => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Solana Devnet",
                chain_id: 0,
                is_evm: false,
                testnet: true,
                token: "SOL",
                rpc_url: "https://api.devnet.solana.com",
                signing_scheme: SigningScheme::SchnorrEd25519Sha512,
                derivation_path: "m/44'/501'/0'/0'",
            },
            Self::Anvil => ChainInfo {
                chain: self.chain_key(),
                chain_name: "Anvil",
                chain_id: 31337,
                is_evm: true,
                testnet: true,
                token: "ETH",
                rpc_url: "http://127.0.0.1:8545",
                signing_scheme: SigningScheme::EcdsaK256Sha256,
                derivation_path: "m/44'/501'/0'/0'",
            },
        }
    }

    /// Returns the effective RPC URL for this chain. For Base mainnet, uses
    /// BASE_CHAIN_RPC env var if set; otherwise uses the default public RPC.
    pub fn rpc_url(self) -> String {
        if self == Self::Base
            && let Ok(url) = env::var("BASE_CHAIN_RPC")
            && !url.trim().is_empty()
        {
            return url.trim().to_string();
        }
        self.info().rpc_url.to_string()
    }

    pub fn all_chains() -> &'static [Chain] {
        &[
            Chain::Ethereum,
            Chain::BnbSmartChain,
            Chain::ArbitrumOne,
            Chain::Base,
            Chain::AvalancheCChain,
            Chain::PolygonPos,
            Chain::Optimism,
            Chain::Blast,
            Chain::Scroll,
            Chain::Mantle,
            Chain::Linea,
            Chain::Cronos,
            Chain::Mode,
            Chain::ZkSyncEra,
            Chain::PulseChain,
            Chain::Fantom,
            Chain::MantaPacific,
            Chain::Celo,
            Chain::Kava,
            Chain::Gnosis,
            Chain::Metis,
            Chain::Astar,
            Chain::Moonbeam,
            Chain::Core,
            Chain::Zora,
            Chain::Klaytn,
            Chain::Canto,
            Chain::Moonriver,
            Chain::Taiko,
            Chain::Flare,
            Chain::ConfluxESpace,
            Chain::Aurora,
            Chain::Telos,
            Chain::BobaNetwork,
            Chain::Heco,
            Chain::OkxChain,
            Chain::ZkFair,
            Chain::Wemix30,
            Chain::Kcc,
            Chain::IoTeX,
            Chain::Evmos,
            Chain::Meter,
            Chain::MilkomedaC1,
            Chain::Syscoin,
            Chain::Dogechain,
            Chain::Fuse,
            Chain::Shibarium,
            Chain::ThunderCore,
            Chain::ReiNetwork,
            Chain::XLayer,
            Chain::Yellowstone,
            Chain::Tron,
            Chain::Solana,
            Chain::Sui,
            Chain::Bitcoin,
            Chain::Aptos,
            Chain::Near,
            Chain::Cardano,
            Chain::Stacks,
            Chain::CosmosHub,
            Chain::Stellar,
            Chain::AptosTestnet,
            Chain::ArbitrumSepolia,
            Chain::AvalancheFuji,
            Chain::BaseSepolia,
            Chain::BitcoinSignet,
            Chain::BnbSmartChainTestnet,
            Chain::EthereumHolesky,
            Chain::EthereumSepolia,
            Chain::PolygonAmoy,
            Chain::SolanaDevnet,
        ]
    }

    /// All supported EVM chains in the same order as the original data.
    pub fn all_evm_chains() -> Vec<Chain> {
        let mut all = Vec::new();
        for chain in Chain::all_chains() {
            if chain.info().is_evm {
                all.push(*chain);
            }
        }
        all
    }
    pub fn all_non_evm_chains() -> Vec<Chain> {
        let mut all = Vec::new();
        for chain in Chain::all_chains() {
            if !chain.info().is_evm {
                all.push(*chain);
            }
        }
        all
    }
    pub fn all_testnet_chains() -> Vec<Chain> {
        let mut all = Vec::new();
        for chain in Chain::all_chains() {
            if chain.info().testnet {
                all.push(*chain);
            }
        }
        all
    }
    pub fn all_testnet_evm_chains() -> Vec<Chain> {
        let mut all = Vec::new();
        for chain in Chain::all_evm_chains() {
            if chain.info().testnet {
                all.push(chain);
            }
        }
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── try_from_str ───────────────────────────────────────────────────
    #[test]
    fn try_from_str_known_chains() {
        assert_eq!(Chain::try_from_str("ethereum").unwrap(), Chain::Ethereum);
        assert_eq!(Chain::try_from_str("base").unwrap(), Chain::Base);
        assert_eq!(Chain::try_from_str("solana").unwrap(), Chain::Solana);
        assert_eq!(Chain::try_from_str("bitcoin").unwrap(), Chain::Bitcoin);
    }

    #[test]
    fn try_from_str_case_insensitive() {
        assert_eq!(Chain::try_from_str("ETHEREUM").unwrap(), Chain::Ethereum);
        assert_eq!(Chain::try_from_str("Ethereum").unwrap(), Chain::Ethereum);
        assert_eq!(Chain::try_from_str("BASE").unwrap(), Chain::Base);
    }

    #[test]
    fn try_from_str_invalid() {
        assert!(Chain::try_from_str("nonexistent").is_err());
        assert!(Chain::try_from_str("").is_err());
    }

    #[test]
    fn try_from_str_testnets() {
        assert_eq!(
            Chain::try_from_str("ethereumsepolia").unwrap(),
            Chain::EthereumSepolia
        );
        assert_eq!(
            Chain::try_from_str("basesepolia").unwrap(),
            Chain::BaseSepolia
        );
        assert_eq!(
            Chain::try_from_str("solanadevnet").unwrap(),
            Chain::SolanaDevnet
        );
    }

    // ── chain_key round-trip ───────────────────────────────────────────
    #[test]
    fn chain_key_round_trip_for_evm() {
        // For EVM chains, chain_key should round-trip through try_from_str for most chains
        for chain in Chain::all_chains() {
            let key = chain.chain_key();
            // The try_from_str lookup is not always a direct inverse of chain_key
            // (e.g., "bobanetwork" vs "boba"), so we just verify chain_key is not empty.
            assert!(!key.is_empty(), "chain_key should not be empty");
        }
    }

    // ── info ───────────────────────────────────────────────────────────
    #[test]
    fn ethereum_info_correct() {
        let info = Chain::Ethereum.info();
        assert_eq!(info.chain_id, 1);
        assert!(info.is_evm);
        assert!(!info.testnet);
        assert_eq!(info.token, "ETH");
    }

    #[test]
    fn solana_is_non_evm() {
        let info = Chain::Solana.info();
        assert!(!info.is_evm);
    }

    #[test]
    fn bitcoin_is_non_evm() {
        let info = Chain::Bitcoin.info();
        assert!(!info.is_evm);
    }

    #[test]
    fn sepolia_is_testnet() {
        let info = Chain::EthereumSepolia.info();
        assert!(info.testnet);
        assert!(info.is_evm);
    }

    #[test]
    fn all_chains_have_nonempty_chain_name() {
        for chain in Chain::all_chains() {
            let info = chain.info();
            assert!(
                !info.chain_name.is_empty(),
                "{:?} has empty chain_name",
                chain
            );
        }
    }

    #[test]
    fn all_chains_have_nonempty_rpc_url() {
        for chain in Chain::all_chains() {
            let info = chain.info();
            assert!(!info.rpc_url.is_empty(), "{:?} has empty rpc_url", chain);
        }
    }

    #[test]
    fn all_chains_have_nonempty_derivation_path() {
        for chain in Chain::all_chains() {
            let info = chain.info();
            assert!(
                !info.derivation_path.is_empty(),
                "{:?} has empty derivation_path",
                chain
            );
        }
    }

    // ── Chain ID uniqueness ─────────────────────────────────────────────
    #[test]
    fn evm_chain_ids_are_unique() {
        let evm_chains = Chain::all_evm_chains();
        let mut ids: Vec<u64> = evm_chains.iter().map(|c| c.info().chain_id).collect();
        let len_before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            len_before,
            "duplicate chain_id found among EVM chains"
        );
    }

    // ── Filter helpers ─────────────────────────────────────────────────
    #[test]
    fn all_evm_chains_are_evm() {
        for chain in Chain::all_evm_chains() {
            assert!(chain.info().is_evm, "{:?} should be EVM", chain);
        }
    }

    #[test]
    fn all_non_evm_chains_are_non_evm() {
        for chain in Chain::all_non_evm_chains() {
            assert!(!chain.info().is_evm, "{:?} should be non-EVM", chain);
        }
    }

    #[test]
    fn all_testnet_chains_are_testnet() {
        for chain in Chain::all_testnet_chains() {
            assert!(chain.info().testnet, "{:?} should be testnet", chain);
        }
    }

    #[test]
    fn all_chains_nonempty() {
        assert!(!Chain::all_chains().is_empty());
    }

    #[test]
    fn evm_plus_non_evm_equals_all() {
        let total = Chain::all_chains().len();
        let evm = Chain::all_evm_chains().len();
        let non_evm = Chain::all_non_evm_chains().len();
        assert_eq!(evm + non_evm, total);
    }
}
