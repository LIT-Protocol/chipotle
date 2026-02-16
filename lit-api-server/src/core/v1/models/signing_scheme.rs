use crate::core::v1::models::curve_type::CurveType;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// Cryptographic signing algorithm types supported by the system.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum SigningAlgorithm {
    /// Pairing-based cryptography (e.g., BLS signatures).
    Pairing,
    /// Elliptic Curve Digital Signature Algorithm.
    Ecdsa,
    /// Schnorr signature scheme.
    Schnorr,
}

/// Preference for public key encoding format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum KeyFormatPreference {
    /// Full uncompressed point representation.
    Uncompressed,
    /// Compressed point representation (x-coordinate with sign bit).
    Compressed,
}

/// Comprehensive signing schemes combining curve type, signature algorithm, and hash function.
///
/// Serializes as a string in JSON (e.g., "EcdsaK256Sha256", "Bls12381", "SchnorrEd25519Sha512").
///
/// Valid values: Bls12381, EcdsaK256Sha256, EcdsaP256Sha256, EcdsaP384Sha384,
/// SchnorrEd25519Sha512, SchnorrK256Sha256, SchnorrP256Sha256, SchnorrP384Sha384,
/// SchnorrRistretto25519Sha512, SchnorrEd448Shake256, SchnorrRedJubjubBlake2b512,
/// SchnorrK256Taproot, SchnorrRedDecaf377Blake2b512, SchnorrRedPallasBlake2b512,
/// SchnorrkelSubstrate, Bls12381G1ProofOfPossession
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SigningScheme {
    /// BLS12-381 pairing-based signatures (default).
    #[default]
    Bls12381,
    /// ECDSA on secp256k1 with SHA-256.
    EcdsaK256Sha256,
    EcdsaP256Sha256,
    EcdsaP384Sha384,
    SchnorrEd25519Sha512,
    SchnorrK256Sha256,
    SchnorrP256Sha256,
    SchnorrP384Sha384,
    SchnorrRistretto25519Sha512,
    SchnorrEd448Shake256,
    SchnorrRedJubjubBlake2b512,
    SchnorrK256Taproot,
    SchnorrRedDecaf377Blake2b512,
    SchnorrRedPallasBlake2b512,
    SchnorrkelSubstrate,
    Bls12381G1ProofOfPossession,
}

impl Display for SigningScheme {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bls12381 => write!(f, "Bls12381"),
            Self::EcdsaK256Sha256 => write!(f, "EcdsaK256Sha256"),
            Self::EcdsaP256Sha256 => write!(f, "EcdsaP256Sha256"),
            Self::EcdsaP384Sha384 => write!(f, "EcdsaP384Sha384"),
            Self::SchnorrEd25519Sha512 => write!(f, "SchnorrEd25519Sha512"),
            Self::SchnorrK256Sha256 => write!(f, "SchnorrK256Sha256"),
            Self::SchnorrP256Sha256 => write!(f, "SchnorrP256Sha256"),
            Self::SchnorrP384Sha384 => write!(f, "SchnorrP384Sha384"),
            Self::SchnorrRistretto25519Sha512 => write!(f, "SchnorrRistretto25519Sha512"),
            Self::SchnorrEd448Shake256 => write!(f, "SchnorrEd448Shake256"),
            Self::SchnorrRedJubjubBlake2b512 => write!(f, "SchnorrRedJubjubBlake2b512"),
            Self::SchnorrRedPallasBlake2b512 => write!(f, "SchnorrRedPallasBlake2b512"),
            Self::SchnorrK256Taproot => write!(f, "SchnorrK256Taproot"),
            Self::SchnorrRedDecaf377Blake2b512 => write!(f, "SchnorrRedDecaf377Blake2b512"),
            Self::SchnorrkelSubstrate => write!(f, "SchnorrkelSubstrate"),
            Self::Bls12381G1ProofOfPossession => write!(f, "Bls12381G1ProofOfPossession"),
        }
    }
}

impl FromStr for SigningScheme {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Bls12381" => Ok(SigningScheme::Bls12381),
            "EcdsaK256Sha256" => Ok(SigningScheme::EcdsaK256Sha256),
            "EcdsaP256Sha256" => Ok(SigningScheme::EcdsaP256Sha256),
            "EcdsaP384Sha384" => Ok(SigningScheme::EcdsaP384Sha384),
            "SchnorrEd25519Sha512" => Ok(SigningScheme::SchnorrEd25519Sha512),
            "SchnorrK256Sha256" => Ok(SigningScheme::SchnorrK256Sha256),
            "SchnorrP256Sha256" => Ok(SigningScheme::SchnorrP256Sha256),
            "SchnorrP384Sha384" => Ok(SigningScheme::SchnorrP384Sha384),
            "SchnorrRistretto25519Sha512" => Ok(SigningScheme::SchnorrRistretto25519Sha512),
            "SchnorrEd448Shake256" => Ok(SigningScheme::SchnorrEd448Shake256),
            "SchnorrRedJubjubBlake2b512" => Ok(SigningScheme::SchnorrRedJubjubBlake2b512),
            "SchnorrRedPallasBlake2b512" => Ok(SigningScheme::SchnorrRedPallasBlake2b512),
            "SchnorrK256Taproot" => Ok(SigningScheme::SchnorrK256Taproot),
            "SchnorrRedDecaf377Blake2b512" => Ok(SigningScheme::SchnorrRedDecaf377Blake2b512),
            "SchnorrkelSubstrate" => Ok(SigningScheme::SchnorrkelSubstrate),
            "Bls12381G1ProofOfPossession" => Ok(SigningScheme::Bls12381G1ProofOfPossession),
            _ => Err(anyhow::anyhow!("Invalid signing scheme: {s}")),
        }
    }
}

impl From<SigningScheme> for u8 {
    fn from(value: SigningScheme) -> Self {
        match value {
            SigningScheme::Bls12381 => 1,
            SigningScheme::EcdsaK256Sha256 => 2,
            SigningScheme::EcdsaP256Sha256 => 3,
            SigningScheme::EcdsaP384Sha384 => 4,
            SigningScheme::SchnorrEd25519Sha512 => 5,
            SigningScheme::SchnorrK256Sha256 => 6,
            SigningScheme::SchnorrP256Sha256 => 7,
            SigningScheme::SchnorrP384Sha384 => 8,
            SigningScheme::SchnorrRistretto25519Sha512 => 9,
            SigningScheme::SchnorrEd448Shake256 => 10,
            SigningScheme::SchnorrRedJubjubBlake2b512 => 11,
            SigningScheme::SchnorrK256Taproot => 12,
            SigningScheme::SchnorrRedDecaf377Blake2b512 => 13,
            SigningScheme::SchnorrkelSubstrate => 14,
            SigningScheme::Bls12381G1ProofOfPossession => 15,
            SigningScheme::SchnorrRedPallasBlake2b512 => 16,
        }
    }
}

impl TryFrom<u8> for SigningScheme {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(SigningScheme::Bls12381),
            2 => Ok(SigningScheme::EcdsaK256Sha256),
            3 => Ok(SigningScheme::EcdsaP256Sha256),
            4 => Ok(SigningScheme::EcdsaP384Sha384),
            5 => Ok(SigningScheme::SchnorrEd25519Sha512),
            6 => Ok(SigningScheme::SchnorrK256Sha256),
            7 => Ok(SigningScheme::SchnorrP256Sha256),
            8 => Ok(SigningScheme::SchnorrP384Sha384),
            9 => Ok(SigningScheme::SchnorrRistretto25519Sha512),
            10 => Ok(SigningScheme::SchnorrEd448Shake256),
            11 => Ok(SigningScheme::SchnorrRedJubjubBlake2b512),
            12 => Ok(SigningScheme::SchnorrK256Taproot),
            13 => Ok(SigningScheme::SchnorrRedDecaf377Blake2b512),
            14 => Ok(SigningScheme::SchnorrkelSubstrate),
            15 => Ok(SigningScheme::Bls12381G1ProofOfPossession),
            16 => Ok(SigningScheme::SchnorrRedPallasBlake2b512),
            _ => Err(anyhow::anyhow!("Invalid signing scheme: {}", value)),
        }
    }
}

impl Serialize for SigningScheme {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_string())
        } else {
            serializer.serialize_u8((*self).into())
        }
    }
}

impl<'de> Deserialize<'de> for SigningScheme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            SigningScheme::from_str(&s).map_err(serde::de::Error::custom)
        } else {
            SigningScheme::try_from(u8::deserialize(deserializer)?)
                .map_err(serde::de::Error::custom)
        }
    }
}

impl SigningScheme {
    pub fn supports_algorithm(&self, algorithm: SigningAlgorithm) -> bool {
        // required to keep the matches aligned like this.
        matches!(
            (algorithm, self),
            (SigningAlgorithm::Pairing, SigningScheme::Bls12381)
                | (
                    SigningAlgorithm::Pairing,
                    SigningScheme::Bls12381G1ProofOfPossession
                )
                | (SigningAlgorithm::Ecdsa, SigningScheme::EcdsaK256Sha256)
                | (
                    SigningAlgorithm::Schnorr,
                    SigningScheme::SchnorrEd25519Sha512
                )
                | (SigningAlgorithm::Schnorr, SigningScheme::SchnorrK256Sha256)
                | (SigningAlgorithm::Schnorr, SigningScheme::SchnorrP256Sha256)
                | (SigningAlgorithm::Schnorr, SigningScheme::SchnorrP384Sha384)
                | (
                    SigningAlgorithm::Schnorr,
                    SigningScheme::SchnorrRistretto25519Sha512
                )
                | (
                    SigningAlgorithm::Schnorr,
                    SigningScheme::SchnorrEd448Shake256
                )
                | (
                    SigningAlgorithm::Schnorr,
                    SigningScheme::SchnorrRedJubjubBlake2b512
                )
                | (SigningAlgorithm::Schnorr, SigningScheme::SchnorrK256Taproot)
                | (
                    SigningAlgorithm::Schnorr,
                    SigningScheme::SchnorrRedDecaf377Blake2b512
                )
                | (
                    SigningAlgorithm::Schnorr,
                    SigningScheme::SchnorrkelSubstrate
                )
                | (
                    SigningAlgorithm::Schnorr,
                    SigningScheme::SchnorrRedPallasBlake2b512
                )
        )
    }

    pub fn supports_curve(&self, curve_type: CurveType) -> bool {
        self.curve_type() == curve_type
    }

    pub fn preferred_format(&self) -> KeyFormatPreference {
        match self {
            Self::Bls12381
            | Self::Bls12381G1ProofOfPossession
            | Self::SchnorrK256Sha256
            | Self::SchnorrP256Sha256
            | Self::SchnorrP384Sha384
            | Self::SchnorrK256Taproot
            | Self::SchnorrEd25519Sha512
            | Self::SchnorrRistretto25519Sha512
            | Self::SchnorrEd448Shake256
            | Self::SchnorrRedJubjubBlake2b512
            | Self::SchnorrRedPallasBlake2b512
            | Self::SchnorrRedDecaf377Blake2b512
            | Self::SchnorrkelSubstrate => KeyFormatPreference::Compressed,
            Self::EcdsaK256Sha256 | Self::EcdsaP256Sha256 | Self::EcdsaP384Sha384 => {
                KeyFormatPreference::Uncompressed
            }
        }
    }

    pub const fn ecdsa_message_len(&self) -> usize {
        match self {
            Self::EcdsaK256Sha256 => 32,
            Self::EcdsaP256Sha256 => 32,
            Self::EcdsaP384Sha384 => 48,
            _ => 0,
        }
    }

    pub const fn curve_type(&self) -> CurveType {
        match self {
            Self::Bls12381 => CurveType::BLS,
            Self::EcdsaK256Sha256 => CurveType::K256,
            Self::EcdsaP256Sha256 => CurveType::P256,
            Self::EcdsaP384Sha384 => CurveType::P384,
            Self::SchnorrEd25519Sha512 => CurveType::Ed25519,
            Self::SchnorrK256Sha256 => CurveType::K256,
            Self::SchnorrP256Sha256 => CurveType::P256,
            Self::SchnorrP384Sha384 => CurveType::P384,
            Self::SchnorrRistretto25519Sha512 | Self::SchnorrkelSubstrate => {
                CurveType::Ristretto25519
            }
            Self::SchnorrEd448Shake256 => CurveType::Ed448,
            Self::SchnorrRedJubjubBlake2b512 => CurveType::RedJubjub,
            Self::SchnorrRedPallasBlake2b512 => CurveType::RedPallas,
            Self::SchnorrK256Taproot => CurveType::K256,
            Self::SchnorrRedDecaf377Blake2b512 => CurveType::RedDecaf377,
            Self::Bls12381G1ProofOfPossession => CurveType::BLS12381G1,
        }
    }

    pub const fn id_sign_ctx(&self) -> &'static [u8] {
        match self {
            SigningScheme::Bls12381 => b"LIT_HD_KEY_ID_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_",
            SigningScheme::EcdsaP256Sha256 | SigningScheme::SchnorrP256Sha256 => {
                b"LIT_HD_KEY_ID_P256_XMD:SHA-256_SSWU_RO_NUL_"
            }
            SigningScheme::SchnorrK256Taproot
            | SigningScheme::EcdsaK256Sha256
            | SigningScheme::SchnorrK256Sha256 => b"LIT_HD_KEY_ID_K256_XMD:SHA-256_SSWU_RO_NUL_",
            SigningScheme::EcdsaP384Sha384 | SigningScheme::SchnorrP384Sha384 => {
                b"LIT_HD_KEY_ID_P384_XMD:SHA-384_SSWU_RO_NUL_"
            }
            SigningScheme::SchnorrRistretto25519Sha512 | SigningScheme::SchnorrkelSubstrate => {
                b"LIT_HD_KEY_ID_RISTRETTO255_XMD:SHA-512_ELL2_RO_NUL_"
            }
            SigningScheme::SchnorrEd25519Sha512 => {
                b"LIT_HD_KEY_ID_ED25519_XMD:SHA-512_ELL2_RO_NUL_"
            }
            SigningScheme::SchnorrEd448Shake256 => {
                b"LIT_HD_KEY_ID_ED448_XOF:SHAKE-256_ELL2_RO_NUL_"
            }
            SigningScheme::SchnorrRedJubjubBlake2b512 => {
                b"LIT_HD_KEY_ID_REDJUBJUB_XMD:BLAKE2B-512_ELL2_RO_NUL_"
            }
            SigningScheme::SchnorrRedPallasBlake2b512 => {
                b"LIT_HD_KEY_ID_REDPALLAS_XMD:BLAKE2B-512_SSWU_RO_NUL_"
            }
            SigningScheme::SchnorrRedDecaf377Blake2b512 => {
                b"LIT_HD_KEY_ID_DECAF377_XMD:BLAKE2B-512_ELL2_RO_NUL_"
            }
            SigningScheme::Bls12381G1ProofOfPossession => {
                b"LIT_HD_KEY_ID_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_"
            }
        }
    }

    pub const fn hash_prior_to_sending(&self) -> bool {
        match self {
            Self::SchnorrK256Sha256
            | Self::SchnorrP256Sha256
            | Self::SchnorrP384Sha384
            | Self::SchnorrEd25519Sha512
            | Self::SchnorrRistretto25519Sha512
            | Self::SchnorrEd448Shake256
            | Self::SchnorrRedJubjubBlake2b512
            | Self::SchnorrRedPallasBlake2b512
            | Self::SchnorrRedDecaf377Blake2b512
            | Self::SchnorrkelSubstrate
            | Self::Bls12381
            | Self::Bls12381G1ProofOfPossession => false,
            Self::EcdsaK256Sha256
            | Self::EcdsaP256Sha256
            | Self::EcdsaP384Sha384
            | Self::SchnorrK256Taproot => true,
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Bls12381 => "Bls12381",
            Self::EcdsaK256Sha256 => "EcdsaK256Sha256",
            Self::EcdsaP256Sha256 => "EcdsaP256Sha256",
            Self::EcdsaP384Sha384 => "EcdsaP384Sha384",
            Self::SchnorrEd25519Sha512 => "SchnorrEd25519Sha512",
            Self::SchnorrK256Sha256 => "SchnorrK256Sha256",
            Self::SchnorrP256Sha256 => "SchnorrP256Sha256",
            Self::SchnorrP384Sha384 => "SchnorrP384Sha384",
            Self::SchnorrRistretto25519Sha512 => "SchnorrRistretto25519Sha512",
            Self::SchnorrEd448Shake256 => "SchnorrEd448Shake256",
            Self::SchnorrRedJubjubBlake2b512 => "SchnorrRedJubjubBlake2b512",
            Self::SchnorrRedPallasBlake2b512 => "SchnorrRedPallasBlake2b512",
            Self::SchnorrK256Taproot => "SchnorrK256Taproot",
            Self::SchnorrRedDecaf377Blake2b512 => "SchnorrRedDecaf377Blake2b512",
            Self::SchnorrkelSubstrate => "SchnorrkelSubstrate",
            Self::Bls12381G1ProofOfPossession => "Bls12381G1ProofOfPossession",
        }
    }
}
