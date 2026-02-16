
use serde::{Deserialize, Serialize};
use std::array::IntoIter;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use anyhow::Result;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum CurveType {
    #[default]
    BLS = 1, // Could be further separated as G1 and G2.
    K256 = 2,           // secp256k1 with the SHA-256 hash function
    Ed25519 = 3,        // Ed25519 with the SHA-512 hash function
    Ed448 = 4,          // Ed448 with the SHAKE-256 hash function
    Ristretto25519 = 5, // Ristretto25519 with the SHA-512 hash function
    P256 = 6,           // NistP256 with SHA-256 hash function
    P384 = 7,           // NistP384 with SHA-384 hash function
    RedJubjub = 8,      // RedJubjub
    RedDecaf377 = 9,    // RedDecaf377
    BLS12381G1 = 10,    // Signatures in G2 while Public Keys in G1
    RedPallas = 11,     // RedPallas
}

impl CurveType {
    pub const NUM_USED_CURVES: usize = 11;

    pub const fn as_str(&self) -> &'static str {
        match self {
            CurveType::BLS => "BLS12381G1",
            CurveType::K256 => "Secp256k1",
            CurveType::P256 => "P256",
            CurveType::P384 => "P384",
            CurveType::Ed25519 => "Ed25519",
            CurveType::Ed448 => "Ed448",
            CurveType::Ristretto25519 => "Ristretto25519",
            CurveType::RedJubjub => "RedJubjub",
            CurveType::RedDecaf377 => "RedDecaf377",
            CurveType::BLS12381G1 => "BLS12381G1Sign",
            CurveType::RedPallas => "RedPallas",
        }
    }

    pub fn into_iter() -> IntoIter<CurveType, { Self::NUM_USED_CURVES }> {
        use CurveType::*;

        [
            BLS,
            K256,
            Ed25519,
            Ed448,
            Ristretto25519,
            P256,
            P384,
            RedJubjub,
            RedDecaf377,
            BLS12381G1,
            RedPallas,
        ]
        .into_iter()
    }

    pub const fn scalar_len(&self) -> usize {
        match self {
            Self::BLS => 32,
            Self::K256 => 32,
            Self::Ed25519 => 32,
            Self::Ed448 => 57,
            Self::Ristretto25519 => 32,
            Self::P256 => 32,
            Self::P384 => 48,
            Self::RedJubjub => 32,
            Self::RedDecaf377 => 32,
            Self::BLS12381G1 => 32,
            Self::RedPallas => 32,
        }
    }

    pub const fn compressed_point_len(&self) -> usize {
        match self {
            Self::BLS => 48,
            Self::K256 => 33,
            Self::Ed25519 => 32,
            Self::Ed448 => 57,
            Self::Ristretto25519 => 32,
            Self::P256 => 33,
            Self::P384 => 49,
            Self::RedJubjub => 32,
            Self::RedDecaf377 => 32,
            Self::BLS12381G1 => 48,
            Self::RedPallas => 32,
        }
    }

    pub const fn vrf_ctx(&self) -> &'static [u8] {
        match self {
            CurveType::BLS => b"BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_VRF",
            CurveType::K256 => b"secp256k1_XMD:SHA-256_SSWU_RO_NUL_VRF",
            CurveType::P256 => b"P256_XMD:SHA-256_SSWU_RO_NUL_VRF",
            CurveType::P384 => b"P384_XMD:SHA-384_SSWU_RO_NUL_VRF",
            CurveType::Ed25519 => b"ed25519_XMD:SHA-512_ELL2_RO_NUL_VRF",
            CurveType::Ed448 => b"ed448_XOF:SHAKE-256_ELL2_RO_NUL_VRF",
            CurveType::Ristretto25519 => b"ristretto255_XMD:SHA-512_ELL2_RO_NUL_VRF",
            CurveType::RedJubjub => b"redjubjub_XMD:BLAKE2B-512_ELL2_RO_NUL_VRF",
            CurveType::RedDecaf377 => b"decaf377_XMD:BLAKE2B-512_ELL2_RO_NUL_VRF",
            CurveType::BLS12381G1 => b"BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_VRF",
            CurveType::RedPallas => b"redpallas_XMD:BLAKE2B-512_SSWU_RO_NUL_VRF",
        }
    }

    pub const fn backup_prefix(&self) -> &'static str {
        match self {
            CurveType::BLS => "bls",
            CurveType::K256 => "k256",
            CurveType::P256 => "p256",
            CurveType::P384 => "p384",
            CurveType::Ed25519 => "ed25519",
            CurveType::Ed448 => "ed448",
            CurveType::Ristretto25519 => "ristretto25519",
            CurveType::RedJubjub => "jubjub",
            CurveType::RedDecaf377 => "decaf377",
            CurveType::BLS12381G1 => "bls12381g1",
            CurveType::RedPallas => "pallas",
        }
    }

    fn invalid() -> Result<Self> {
        Err(anyhow::anyhow!("invalid curve type"))
    }
}

impl Display for CurveType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CurveType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().as_str() {
            "BLS12381G1" => Ok(CurveType::BLS),
            "ECDSA_CAIT_SITH" | "SECP256K1" => Ok(CurveType::K256),
            "ED25519" => Ok(CurveType::Ed25519),
            "ED448" => Ok(CurveType::Ed448),
            "RISTRETTO25519" => Ok(CurveType::Ristretto25519),
            "P256" => Ok(CurveType::P256),
            "P384" => Ok(CurveType::P384),
            "REDJUBJUB" => Ok(CurveType::RedJubjub),
            "REDDECAF377" => Ok(CurveType::RedDecaf377),
            "BLS12381G1SIGN" => Ok(CurveType::BLS12381G1),
            "REDPALLAS" => Ok(CurveType::RedPallas),
            _ => CurveType::invalid(),
        }
    }
}

impl TryFrom<ethers::types::U256> for CurveType {
    type Error = anyhow::Error;
    fn try_from(curve_type: ethers::types::U256) -> Result<Self> {
        let curve_type = curve_type.as_u32();
        let curve_type = TryInto::<u8>::try_into(curve_type);
        match curve_type {
            Ok(1) => Ok(CurveType::BLS),
            Ok(2) => Ok(CurveType::K256),
            Ok(3) => Ok(CurveType::Ed25519),
            Ok(4) => Ok(CurveType::Ed448),
            Ok(5) => Ok(CurveType::Ristretto25519),
            Ok(6) => Ok(CurveType::P256),
            Ok(7) => Ok(CurveType::P384),
            Ok(8) => Ok(CurveType::RedJubjub),
            Ok(9) => Ok(CurveType::RedDecaf377),
            Ok(10) => Ok(CurveType::BLS12381G1),
            Ok(11) => Ok(CurveType::RedPallas),
            _ => CurveType::invalid(),
        }
    }
}

impl TryFrom<u8> for CurveType {
    type Error = anyhow::Error;
    fn try_from(byte: u8) -> std::result::Result<Self, Self::Error> {
        match byte {
            1 => Ok(CurveType::BLS),
            2 => Ok(CurveType::K256),
            3 => Ok(CurveType::Ed25519),
            4 => Ok(CurveType::Ed448),
            5 => Ok(CurveType::Ristretto25519),
            6 => Ok(CurveType::P256),
            7 => Ok(CurveType::P384),
            8 => Ok(CurveType::RedJubjub),
            9 => Ok(CurveType::RedDecaf377),
            10 => Ok(CurveType::BLS12381G1),
            11 => Ok(CurveType::RedPallas),
            _ => CurveType::invalid(),
        }
    }
}

impl From<CurveType> for ethers::types::U256 {
    fn from(curve_type: CurveType) -> ethers::types::U256 {
        let curve_type = curve_type as u8;
        ethers::types::U256::from(curve_type as u32)
    }
}
