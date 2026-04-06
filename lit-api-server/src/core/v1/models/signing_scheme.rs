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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_SCHEMES: [SigningScheme; 16] = [
        SigningScheme::Bls12381,
        SigningScheme::EcdsaK256Sha256,
        SigningScheme::EcdsaP256Sha256,
        SigningScheme::EcdsaP384Sha384,
        SigningScheme::SchnorrEd25519Sha512,
        SigningScheme::SchnorrK256Sha256,
        SigningScheme::SchnorrP256Sha256,
        SigningScheme::SchnorrP384Sha384,
        SigningScheme::SchnorrRistretto25519Sha512,
        SigningScheme::SchnorrEd448Shake256,
        SigningScheme::SchnorrRedJubjubBlake2b512,
        SigningScheme::SchnorrK256Taproot,
        SigningScheme::SchnorrRedDecaf377Blake2b512,
        SigningScheme::SchnorrkelSubstrate,
        SigningScheme::Bls12381G1ProofOfPossession,
        SigningScheme::SchnorrRedPallasBlake2b512,
    ];

    // ── ALL_SCHEMES exhaustiveness ──────────────────────────────────────
    #[test]
    fn all_schemes_covers_all_u8_variants() {
        // Verify ALL_SCHEMES is exhaustive: every valid u8 value (1..=16) must
        // appear in ALL_SCHEMES. If a new variant is added, this test fails.
        let scheme_bytes: Vec<u8> = ALL_SCHEMES.iter().map(|s| (*s).into()).collect();
        for i in 1u8..=16 {
            assert!(
                scheme_bytes.contains(&i),
                "ALL_SCHEMES missing variant with u8 value {i}"
            );
        }
        assert_eq!(
            ALL_SCHEMES.len(),
            16,
            "ALL_SCHEMES count mismatch; update if variants were added"
        );
    }

    // ── FromStr round-trip ──────────────────────────────────────────────
    #[test]
    fn from_str_round_trip_all() {
        for scheme in ALL_SCHEMES {
            let s = scheme.to_string();
            let parsed = SigningScheme::from_str(&s).unwrap();
            assert_eq!(scheme, parsed, "round-trip failed for {s}");
        }
    }

    #[test]
    fn from_str_invalid() {
        assert!(SigningScheme::from_str("NonexistentScheme").is_err());
    }

    // ── u8 round-trip ──────────────────────────────────────────────────
    #[test]
    fn u8_round_trip_all() {
        for scheme in ALL_SCHEMES {
            let byte: u8 = scheme.into();
            let back = SigningScheme::try_from(byte).unwrap();
            assert_eq!(scheme, back, "u8 round-trip failed for {scheme}");
        }
    }

    #[test]
    fn try_from_u8_invalid() {
        assert!(SigningScheme::try_from(0u8).is_err());
        assert!(SigningScheme::try_from(17u8).is_err());
        assert!(SigningScheme::try_from(255u8).is_err());
    }

    #[test]
    fn u8_values_are_contiguous_1_through_16() {
        let mut bytes: Vec<u8> = ALL_SCHEMES.iter().map(|s| (*s).into()).collect();
        bytes.sort();
        assert_eq!(bytes, (1u8..=16).collect::<Vec<_>>());
    }

    // ── Serde JSON (human-readable) ─────────────────────────────────────
    #[test]
    fn serde_json_round_trip() {
        for scheme in ALL_SCHEMES {
            let json = serde_json::to_string(&scheme).unwrap();
            // Should serialize as a quoted string, not a number.
            assert!(json.starts_with('"'), "expected string for {scheme}");
            let back: SigningScheme = serde_json::from_str(&json).unwrap();
            assert_eq!(scheme, back);
        }
    }

    // ── Serde binary (non-human-readable) ──────────────────────────────
    #[test]
    fn serde_bare_round_trip() {
        for scheme in ALL_SCHEMES {
            let bytes = serde_bare::to_vec(&scheme).unwrap();
            let back: SigningScheme = serde_bare::from_slice(&bytes).unwrap();
            assert_eq!(scheme, back);
        }
    }

    // ── supports_algorithm ─────────────────────────────────────────────
    #[test]
    fn bls_supports_pairing() {
        assert!(SigningScheme::Bls12381.supports_algorithm(SigningAlgorithm::Pairing));
        assert!(!SigningScheme::Bls12381.supports_algorithm(SigningAlgorithm::Ecdsa));
        assert!(
            SigningScheme::Bls12381G1ProofOfPossession
                .supports_algorithm(SigningAlgorithm::Pairing)
        );
    }

    #[test]
    fn ecdsa_k256_supports_ecdsa() {
        assert!(SigningScheme::EcdsaK256Sha256.supports_algorithm(SigningAlgorithm::Ecdsa));
        assert!(!SigningScheme::EcdsaK256Sha256.supports_algorithm(SigningAlgorithm::Schnorr));
    }

    #[test]
    fn schnorr_supports_schnorr() {
        assert!(SigningScheme::SchnorrEd25519Sha512.supports_algorithm(SigningAlgorithm::Schnorr));
        assert!(!SigningScheme::SchnorrEd25519Sha512.supports_algorithm(SigningAlgorithm::Pairing));
    }

    // ── supports_curve ─────────────────────────────────────────────────
    #[test]
    fn supports_own_curve() {
        for scheme in ALL_SCHEMES {
            assert!(
                scheme.supports_curve(scheme.curve_type()),
                "{scheme} does not support its own curve"
            );
        }
    }

    // ── preferred_format ───────────────────────────────────────────────
    #[test]
    fn ecdsa_prefers_uncompressed() {
        assert_eq!(
            SigningScheme::EcdsaK256Sha256.preferred_format(),
            KeyFormatPreference::Uncompressed
        );
        assert_eq!(
            SigningScheme::EcdsaP256Sha256.preferred_format(),
            KeyFormatPreference::Uncompressed
        );
        assert_eq!(
            SigningScheme::EcdsaP384Sha384.preferred_format(),
            KeyFormatPreference::Uncompressed
        );
    }

    #[test]
    fn non_ecdsa_prefers_compressed() {
        let ecdsa_schemes = [
            SigningScheme::EcdsaK256Sha256,
            SigningScheme::EcdsaP256Sha256,
            SigningScheme::EcdsaP384Sha384,
        ];
        for scheme in ALL_SCHEMES {
            if !ecdsa_schemes.contains(&scheme) {
                assert_eq!(
                    scheme.preferred_format(),
                    KeyFormatPreference::Compressed,
                    "{scheme} should prefer compressed"
                );
            }
        }
    }

    // ── ecdsa_message_len ──────────────────────────────────────────────
    #[test]
    fn ecdsa_message_len_nonzero_for_ecdsa() {
        assert_eq!(SigningScheme::EcdsaK256Sha256.ecdsa_message_len(), 32);
        assert_eq!(SigningScheme::EcdsaP256Sha256.ecdsa_message_len(), 32);
        assert_eq!(SigningScheme::EcdsaP384Sha384.ecdsa_message_len(), 48);
    }

    #[test]
    fn ecdsa_message_len_zero_for_non_ecdsa() {
        assert_eq!(SigningScheme::Bls12381.ecdsa_message_len(), 0);
        assert_eq!(SigningScheme::SchnorrEd25519Sha512.ecdsa_message_len(), 0);
    }

    // ── hash_prior_to_sending ──────────────────────────────────────────
    #[test]
    fn hash_prior_true_for_ecdsa_and_taproot() {
        assert!(SigningScheme::EcdsaK256Sha256.hash_prior_to_sending());
        assert!(SigningScheme::EcdsaP256Sha256.hash_prior_to_sending());
        assert!(SigningScheme::EcdsaP384Sha384.hash_prior_to_sending());
        assert!(SigningScheme::SchnorrK256Taproot.hash_prior_to_sending());
    }

    #[test]
    fn hash_prior_false_for_schnorr_and_bls() {
        assert!(!SigningScheme::Bls12381.hash_prior_to_sending());
        assert!(!SigningScheme::SchnorrEd25519Sha512.hash_prior_to_sending());
        assert!(!SigningScheme::SchnorrK256Sha256.hash_prior_to_sending());
    }

    // ── id_sign_ctx ────────────────────────────────────────────────────
    #[test]
    fn id_sign_ctx_starts_with_lit_hd_key() {
        for scheme in ALL_SCHEMES {
            let ctx = scheme.id_sign_ctx();
            assert!(
                ctx.starts_with(b"LIT_HD_KEY_ID_"),
                "{scheme}: id_sign_ctx does not start with LIT_HD_KEY_ID_"
            );
        }
    }

    // ── as_str / Display ───────────────────────────────────────────────
    #[test]
    fn as_str_matches_display() {
        for scheme in ALL_SCHEMES {
            assert_eq!(scheme.as_str(), &scheme.to_string());
        }
    }

    // ── Default ────────────────────────────────────────────────────────
    #[test]
    fn default_is_bls12381() {
        assert_eq!(SigningScheme::default(), SigningScheme::Bls12381);
    }
}
