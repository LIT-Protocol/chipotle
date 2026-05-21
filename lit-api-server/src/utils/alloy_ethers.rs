//! Phase 4 bridge: alloy ↔ ethers primitive conversion helpers.
//!
//! Removed in Phase 5, when the abigen-generated `account_config_contract`
//! bindings are regenerated via `sol!` and stop returning ethers types.
//! Until then, alloy-typed callers (`accounts/mod.rs`, `account_management.rs`)
//! convert at the contract call site.

use alloy::primitives::{Address, U256 as AlloyU256};
use ethers::types::{H160, U256 as EthersU256};

#[inline]
pub fn alloy_u256_to_ethers(x: AlloyU256) -> EthersU256 {
    EthersU256::from_big_endian(&x.to_be_bytes::<32>())
}

#[inline]
pub fn ethers_u256_to_alloy(x: EthersU256) -> AlloyU256 {
    let mut bytes = [0u8; 32];
    x.to_big_endian(&mut bytes);
    AlloyU256::from_be_bytes(bytes)
}

#[inline]
pub fn alloy_address_to_ethers(a: Address) -> H160 {
    H160::from_slice(a.as_slice())
}

#[inline]
pub fn ethers_address_to_alloy(h: H160) -> Address {
    Address::from_slice(h.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u256_roundtrip() {
        let cases = [
            AlloyU256::ZERO,
            AlloyU256::from(1u64),
            AlloyU256::from(u64::MAX),
            AlloyU256::MAX,
        ];
        for v in cases {
            assert_eq!(ethers_u256_to_alloy(alloy_u256_to_ethers(v)), v);
        }
    }

    #[test]
    fn address_roundtrip() {
        let cases = [
            Address::ZERO,
            Address::from([0xffu8; 20]),
            Address::from([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0xde, 0xad, 0xbe, 0xef]),
        ];
        for a in cases {
            assert_eq!(ethers_address_to_alloy(alloy_address_to_ethers(a)), a);
        }
    }
}
