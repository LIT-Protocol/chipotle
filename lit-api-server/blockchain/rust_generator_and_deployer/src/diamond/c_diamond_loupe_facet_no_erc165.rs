pub use diamond_loupe_facet_no_erc165::*;
/// This module was auto-generated with ethers-rs Abigen.
/// More information at: <https://github.com/gakonst/ethers-rs>
#[allow(
    clippy::enum_variant_names,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms,
    clippy::type_complexity,
    dead_code,
    non_camel_case_types
)]
pub mod diamond_loupe_facet_no_erc165 {
    const _: () = {
        ::core::include_bytes!("./DiamondLoupeFacetNoERC165.json",);
    };
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("facetAddress"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("facetAddress"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("_functionSelector"),
                            kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(4usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes4"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("facetAddress_"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("facetAddresses"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("facetAddresses"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("facetAddresses_"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address[]"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("facetFunctionSelectors"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("facetFunctionSelectors",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("_facet"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("_facetFunctionSelectors",),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::FixedBytes(4usize),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes4[]"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("facets"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("facets"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("facets_"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                        ::ethers::core::abi::ethabi::ParamType::Address,
                                        ::ethers::core::abi::ethabi::ParamType::Array(
                                            ::std::boxed::Box::new(
                                                ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                                    4usize
                                                ),
                                            ),
                                        ),
                                    ],),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct IDiamondLoupe.Facet[]",),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::std::collections::BTreeMap::new(),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static DIAMONDLOUPEFACETNOERC165_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80\x80`@R4`\x15Wa\x07\xE1\x90\x81a\0\x1B\x829\xF3[`\0\x80\xFD\xFE`\x80`@R`\x046\x10\x15a\0\x12W`\0\x80\xFD[`\x005`\xE0\x1C\x80cR\xEFk,\x14a\0WW\x80cz\x0E\xD6'\x14a\0RW\x80c\xAD\xFC\xA1^\x14a\0MWc\xCD\xFF\xAC\xC6\x14a\0HW`\0\x80\xFD[a\x05OV[a\x04\x8EV[a\x02mV[4a\x01oW`\x006`\x03\x19\x01\x12a\x01oW`\0\x80Q` a\x07\x8C\x839\x81Q\x91RTa\0\x81\x81a\x05\xECV[`\0\x80\x92[\x80\x84\x10a\0\xA2W\x81\x83R`@Q\x80a\0\x9E\x85\x82a\x01tV[\x03\x90\xF3[\x90a\0\xD4a\0\xC7a\0\xC2a\0\xB5\x87a\x06/V[\x90T\x90`\x03\x1B\x1C`\xE0\x1B\x90V[a\x06\x8FV[T`\x01`\x01`\xA0\x1B\x03\x16\x90V[`\0`\x01`\x01`\xA0\x1B\x03\x82\x16\x81[\x84\x81\x10a\x01,W[PPa\x01\"W\x81a\x01\x13a\x01\x18\x92a\x01\x04`\x01\x95\x88a\x06\xD4V[`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x90RV[a\x06\xFEV[\x93[\x01\x92\x90a\0\x86V[P\x92`\x01\x90a\x01\x1AV[a\x01Ua\x01Ia\x01<\x83\x8Aa\x06\xD4V[Q`\x01`\x01`\xA0\x1B\x03\x16\x90V[`\x01`\x01`\xA0\x1B\x03\x16\x90V[\x82\x14a\x01cW`\x01\x01a\0\xE2V[PPP`\x01\x86\x80a\0\xEAV[`\0\x80\xFD[` `@\x81\x83\x01\x92\x82\x81R\x84Q\x80\x94R\x01\x92\x01\x90`\0[\x81\x81\x10a\x01\x98WPPP\x90V[\x82Q`\x01`\x01`\xA0\x1B\x03\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x01a\x01\x8BV[\x90` \x80\x83Q\x92\x83\x81R\x01\x92\x01\x90`\0[\x81\x81\x10a\x01\xD5WPPP\x90V[\x82Q`\x01`\x01`\xE0\x1B\x03\x19\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x01a\x01\xC8V[` \x81\x01` \x82R\x82Q\x80\x91R`@\x82\x01\x91` `@\x83`\x05\x1B\x83\x01\x01\x94\x01\x92`\0\x91[\x83\x83\x10a\x02(WPPPPP\x90V[\x90\x91\x92\x93\x94` \x80a\x02^`\x01\x93`?\x19\x86\x82\x03\x01\x87R`@\x83\x8BQ\x87\x80`\xA0\x1B\x03\x81Q\x16\x84R\x01Q\x91\x81\x85\x82\x01R\x01\x90a\x01\xB7V[\x97\x01\x93\x01\x93\x01\x91\x93\x92\x90a\x02\x19V[4a\x01oW`\x006`\x03\x19\x01\x12a\x01oW`\0\x80Q` a\x07\x8C\x839\x81Q\x91RTa\x02\x97\x81a\x07\x12V[a\x02\xA0\x82a\x05\xECV[\x91`\0\x90`\0\x90[\x80\x82\x10a\x03\x04WPP`\0[\x81\x81\x10a\x02\xCCW\x81\x83R`@Q\x80a\0\x9E\x85\x82a\x01\xF5V[\x80a\x02\xEEa\x02\xE7a\x02\xDF`\x01\x94\x88a\x06\xD4V[Qa\xFF\xFF\x16\x90V[a\xFF\xFF\x16\x90V[` a\x02\xFA\x83\x87a\x06\xD4V[Q\x01QR\x01a\x02\xB4V[\x90\x91a\x03\x12a\0\xB5\x84a\x06/V[a\x03\x1Ea\0\xC7\x82a\x06\x8FV[`\0\x80`\x01`\x01`\xA0\x1B\x03\x83\x16[\x85\x82\x10a\x03\xC8W[PPa\x03\xBDW\x91a\x03\xA1a\x03\xB4\x92a\x03b`\x01\x95a\x03R\x85\x8Ba\x06\xD4V[Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x90RV[a\x03k\x86a\x05\xECV[` a\x03w\x85\x8Ba\x06\xD4V[Q\x01Ra\x03\x91` a\x03\x89\x85\x8Ba\x06\xD4V[Q\x01Qa\x06\xC7V[`\x01`\x01`\xE0\x1B\x03\x19\x90\x91\x16\x90RV[a\x01\x13a\x03\xAE\x82\x89a\x06\xD4V[`\x01\x90RV[\x92[\x01\x90a\x02\xA8V[PP\x91`\x01\x90a\x03\xB6V[\x80a\x03\xF2a\x01Ia\x03\xE4\x85\x8D\x98\x9C\x9D\x9E\x97\x96\x9E\x9B\x99\x9A\x9Ba\x06\xD4V[QQ`\x01`\x01`\xA0\x1B\x03\x16\x90V[\x14a\x04\rW`\x01\x80\x9A\x01\x91\x92\x99P\x97\x96\x92\x97\x95\x94\x93\x95a\x03,V[PP\x96\x80a\x04I\x85a\x03\x91\x8Ba\x04Ca\x02\xE7a\x02\xDF\x87\x8A\x9F\x9E\x9Aa\x04:\x9C\x9E\x9D\x9Ca\x04q\x9B` \x92a\x06\xD4V[Q\x01Q\x94a\x06\xD4V[\x90a\x06\xD4V[a\x04ha\x04aa\x04\\a\x02\xDF\x84\x8Da\x06\xD4V[a\x07xV[\x91\x8Aa\x06\xD4V[\x90a\xFF\xFF\x16\x90RV[`\x018\x80a\x034V[\x90` a\x04\x8B\x92\x81\x81R\x01\x90a\x01\xB7V[\x90V[4a\x01oW` 6`\x03\x19\x01\x12a\x01oW`\x045`\x01`\x01`\xA0\x1B\x03\x81\x16\x90\x81\x90\x03a\x01oW`\0\x80Q` a\x07\x8C\x839\x81Q\x91RT`\0a\x04\xCF\x82a\x05\xECV[\x91`\0[\x81\x81\x10a\x04\xEBW\x82\x84R`@Q\x80a\0\x9E\x86\x82a\x04zV[a\x04\xF4\x81a\x06/V[\x90T`\x03\x91\x90\x91\x1B\x1C`\xE0\x1B`\x01`\x01`\xA0\x1B\x03a\x05\x11\x82a\x06\x8FV[T\x16\x86\x14a\x05#W[P`\x01\x01a\x04\xD3V[\x83a\x05H\x91a\x055`\x01\x94\x96\x88a\x06\xD4V[`\x01`\x01`\xE0\x1B\x03\x19\x90\x91\x16\x90Ra\x06\xFEV[\x92\x90a\x05\x1AV[4a\x01oW` 6`\x03\x19\x01\x12a\x01oW`\x045`\x01`\x01`\xE0\x1B\x03\x19\x81\x16\x81\x03a\x01oW` \x90`\x01`\x01`\xA0\x1B\x03\x90a\x05\x89\x90a\x06\x8FV[T\x16`@Q\x90\x81R\xF3[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`@Q\x91\x90`\x1F\x01`\x1F\x19\x16\x82\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x83\x82\x10\x17a\x05\xCFW`@RV[a\x05\x93V[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x05\xCFW`\x05\x1B` \x01\x90V[\x90a\x05\xFEa\x05\xF9\x83a\x05\xD4V[a\x05\xA9V[\x82\x81R\x80\x92a\x06\x0F`\x1F\x19\x91a\x05\xD4V[\x01\x90` 6\x91\x017V[cNH{q`\xE0\x1B`\0R`2`\x04R`$`\0\xFD[\x90`\0\x80Q` a\x07\x8C\x839\x81Q\x91RT\x82\x10\x15a\x06\x8AW`\0\x80Q` a\x07\x8C\x839\x81Q\x91R`\0R`\x03\x82\x90\x1C\x7F\xC0\xD7'a\x0E\xA1bA\xEF\xF4D}\x08\xBB\x1BE\x95\xF7\xD2\xECE\x15($7\xA1;}\r\xF4\xB9\"\x01\x91`\x02\x1B`\x1C\x16\x90V[a\x06\x19V[c\xFF\xFF\xFF\xFF`\xE0\x1B\x16`\0R\x7F\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1C` R`@`\0 \x90V[\x80Q\x15a\x06\x8AW` \x01\x90V[\x80Q\x82\x10\x15a\x06\x8AW` \x91`\x05\x1B\x01\x01\x90V[cNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[`\0\x19\x81\x14a\x07\rW`\x01\x01\x90V[a\x06\xE8V[\x90a\x07\x1Fa\x05\xF9\x83a\x05\xD4V[\x82\x81R\x80\x92a\x070`\x1F\x19\x91a\x05\xD4V[\x01`\0[\x81\x81\x10a\x07@WPPPV[`@Q\x90`@\x82\x01\x91\x80\x83\x10g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x84\x11\x17a\x05\xCFW` \x92`@R`\0\x81R``\x83\x82\x01R\x82\x82\x86\x01\x01R\x01a\x074V[a\xFF\xFF\x16a\xFF\xFF\x81\x14a\x07\rW`\x01\x01\x90V\xFE\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1D\xA2dipfsX\"\x12 vl5\x90\xF6\xAA\x1F;T\t\x9At\xF68\x16\"$\xC7Ey\x92\x96\xB2\x1E\xA9.\x19byR\x10\xB4dsolcC\0\x08\x1C\x003";
    /// The bytecode of the contract.
    pub static DIAMONDLOUPEFACETNOERC165_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\x046\x10\x15a\0\x12W`\0\x80\xFD[`\x005`\xE0\x1C\x80cR\xEFk,\x14a\0WW\x80cz\x0E\xD6'\x14a\0RW\x80c\xAD\xFC\xA1^\x14a\0MWc\xCD\xFF\xAC\xC6\x14a\0HW`\0\x80\xFD[a\x05OV[a\x04\x8EV[a\x02mV[4a\x01oW`\x006`\x03\x19\x01\x12a\x01oW`\0\x80Q` a\x07\x8C\x839\x81Q\x91RTa\0\x81\x81a\x05\xECV[`\0\x80\x92[\x80\x84\x10a\0\xA2W\x81\x83R`@Q\x80a\0\x9E\x85\x82a\x01tV[\x03\x90\xF3[\x90a\0\xD4a\0\xC7a\0\xC2a\0\xB5\x87a\x06/V[\x90T\x90`\x03\x1B\x1C`\xE0\x1B\x90V[a\x06\x8FV[T`\x01`\x01`\xA0\x1B\x03\x16\x90V[`\0`\x01`\x01`\xA0\x1B\x03\x82\x16\x81[\x84\x81\x10a\x01,W[PPa\x01\"W\x81a\x01\x13a\x01\x18\x92a\x01\x04`\x01\x95\x88a\x06\xD4V[`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x90RV[a\x06\xFEV[\x93[\x01\x92\x90a\0\x86V[P\x92`\x01\x90a\x01\x1AV[a\x01Ua\x01Ia\x01<\x83\x8Aa\x06\xD4V[Q`\x01`\x01`\xA0\x1B\x03\x16\x90V[`\x01`\x01`\xA0\x1B\x03\x16\x90V[\x82\x14a\x01cW`\x01\x01a\0\xE2V[PPP`\x01\x86\x80a\0\xEAV[`\0\x80\xFD[` `@\x81\x83\x01\x92\x82\x81R\x84Q\x80\x94R\x01\x92\x01\x90`\0[\x81\x81\x10a\x01\x98WPPP\x90V[\x82Q`\x01`\x01`\xA0\x1B\x03\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x01a\x01\x8BV[\x90` \x80\x83Q\x92\x83\x81R\x01\x92\x01\x90`\0[\x81\x81\x10a\x01\xD5WPPP\x90V[\x82Q`\x01`\x01`\xE0\x1B\x03\x19\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x01a\x01\xC8V[` \x81\x01` \x82R\x82Q\x80\x91R`@\x82\x01\x91` `@\x83`\x05\x1B\x83\x01\x01\x94\x01\x92`\0\x91[\x83\x83\x10a\x02(WPPPPP\x90V[\x90\x91\x92\x93\x94` \x80a\x02^`\x01\x93`?\x19\x86\x82\x03\x01\x87R`@\x83\x8BQ\x87\x80`\xA0\x1B\x03\x81Q\x16\x84R\x01Q\x91\x81\x85\x82\x01R\x01\x90a\x01\xB7V[\x97\x01\x93\x01\x93\x01\x91\x93\x92\x90a\x02\x19V[4a\x01oW`\x006`\x03\x19\x01\x12a\x01oW`\0\x80Q` a\x07\x8C\x839\x81Q\x91RTa\x02\x97\x81a\x07\x12V[a\x02\xA0\x82a\x05\xECV[\x91`\0\x90`\0\x90[\x80\x82\x10a\x03\x04WPP`\0[\x81\x81\x10a\x02\xCCW\x81\x83R`@Q\x80a\0\x9E\x85\x82a\x01\xF5V[\x80a\x02\xEEa\x02\xE7a\x02\xDF`\x01\x94\x88a\x06\xD4V[Qa\xFF\xFF\x16\x90V[a\xFF\xFF\x16\x90V[` a\x02\xFA\x83\x87a\x06\xD4V[Q\x01QR\x01a\x02\xB4V[\x90\x91a\x03\x12a\0\xB5\x84a\x06/V[a\x03\x1Ea\0\xC7\x82a\x06\x8FV[`\0\x80`\x01`\x01`\xA0\x1B\x03\x83\x16[\x85\x82\x10a\x03\xC8W[PPa\x03\xBDW\x91a\x03\xA1a\x03\xB4\x92a\x03b`\x01\x95a\x03R\x85\x8Ba\x06\xD4V[Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x90RV[a\x03k\x86a\x05\xECV[` a\x03w\x85\x8Ba\x06\xD4V[Q\x01Ra\x03\x91` a\x03\x89\x85\x8Ba\x06\xD4V[Q\x01Qa\x06\xC7V[`\x01`\x01`\xE0\x1B\x03\x19\x90\x91\x16\x90RV[a\x01\x13a\x03\xAE\x82\x89a\x06\xD4V[`\x01\x90RV[\x92[\x01\x90a\x02\xA8V[PP\x91`\x01\x90a\x03\xB6V[\x80a\x03\xF2a\x01Ia\x03\xE4\x85\x8D\x98\x9C\x9D\x9E\x97\x96\x9E\x9B\x99\x9A\x9Ba\x06\xD4V[QQ`\x01`\x01`\xA0\x1B\x03\x16\x90V[\x14a\x04\rW`\x01\x80\x9A\x01\x91\x92\x99P\x97\x96\x92\x97\x95\x94\x93\x95a\x03,V[PP\x96\x80a\x04I\x85a\x03\x91\x8Ba\x04Ca\x02\xE7a\x02\xDF\x87\x8A\x9F\x9E\x9Aa\x04:\x9C\x9E\x9D\x9Ca\x04q\x9B` \x92a\x06\xD4V[Q\x01Q\x94a\x06\xD4V[\x90a\x06\xD4V[a\x04ha\x04aa\x04\\a\x02\xDF\x84\x8Da\x06\xD4V[a\x07xV[\x91\x8Aa\x06\xD4V[\x90a\xFF\xFF\x16\x90RV[`\x018\x80a\x034V[\x90` a\x04\x8B\x92\x81\x81R\x01\x90a\x01\xB7V[\x90V[4a\x01oW` 6`\x03\x19\x01\x12a\x01oW`\x045`\x01`\x01`\xA0\x1B\x03\x81\x16\x90\x81\x90\x03a\x01oW`\0\x80Q` a\x07\x8C\x839\x81Q\x91RT`\0a\x04\xCF\x82a\x05\xECV[\x91`\0[\x81\x81\x10a\x04\xEBW\x82\x84R`@Q\x80a\0\x9E\x86\x82a\x04zV[a\x04\xF4\x81a\x06/V[\x90T`\x03\x91\x90\x91\x1B\x1C`\xE0\x1B`\x01`\x01`\xA0\x1B\x03a\x05\x11\x82a\x06\x8FV[T\x16\x86\x14a\x05#W[P`\x01\x01a\x04\xD3V[\x83a\x05H\x91a\x055`\x01\x94\x96\x88a\x06\xD4V[`\x01`\x01`\xE0\x1B\x03\x19\x90\x91\x16\x90Ra\x06\xFEV[\x92\x90a\x05\x1AV[4a\x01oW` 6`\x03\x19\x01\x12a\x01oW`\x045`\x01`\x01`\xE0\x1B\x03\x19\x81\x16\x81\x03a\x01oW` \x90`\x01`\x01`\xA0\x1B\x03\x90a\x05\x89\x90a\x06\x8FV[T\x16`@Q\x90\x81R\xF3[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`@Q\x91\x90`\x1F\x01`\x1F\x19\x16\x82\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x83\x82\x10\x17a\x05\xCFW`@RV[a\x05\x93V[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x05\xCFW`\x05\x1B` \x01\x90V[\x90a\x05\xFEa\x05\xF9\x83a\x05\xD4V[a\x05\xA9V[\x82\x81R\x80\x92a\x06\x0F`\x1F\x19\x91a\x05\xD4V[\x01\x90` 6\x91\x017V[cNH{q`\xE0\x1B`\0R`2`\x04R`$`\0\xFD[\x90`\0\x80Q` a\x07\x8C\x839\x81Q\x91RT\x82\x10\x15a\x06\x8AW`\0\x80Q` a\x07\x8C\x839\x81Q\x91R`\0R`\x03\x82\x90\x1C\x7F\xC0\xD7'a\x0E\xA1bA\xEF\xF4D}\x08\xBB\x1BE\x95\xF7\xD2\xECE\x15($7\xA1;}\r\xF4\xB9\"\x01\x91`\x02\x1B`\x1C\x16\x90V[a\x06\x19V[c\xFF\xFF\xFF\xFF`\xE0\x1B\x16`\0R\x7F\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1C` R`@`\0 \x90V[\x80Q\x15a\x06\x8AW` \x01\x90V[\x80Q\x82\x10\x15a\x06\x8AW` \x91`\x05\x1B\x01\x01\x90V[cNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[`\0\x19\x81\x14a\x07\rW`\x01\x01\x90V[a\x06\xE8V[\x90a\x07\x1Fa\x05\xF9\x83a\x05\xD4V[\x82\x81R\x80\x92a\x070`\x1F\x19\x91a\x05\xD4V[\x01`\0[\x81\x81\x10a\x07@WPPPV[`@Q\x90`@\x82\x01\x91\x80\x83\x10g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x84\x11\x17a\x05\xCFW` \x92`@R`\0\x81R``\x83\x82\x01R\x82\x82\x86\x01\x01R\x01a\x074V[a\xFF\xFF\x16a\xFF\xFF\x81\x14a\x07\rW`\x01\x01\x90V\xFE\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1D\xA2dipfsX\"\x12 vl5\x90\xF6\xAA\x1F;T\t\x9At\xF68\x16\"$\xC7Ey\x92\x96\xB2\x1E\xA9.\x19byR\x10\xB4dsolcC\0\x08\x1C\x003";
    /// The deployed bytecode of the contract.
    pub static DIAMONDLOUPEFACETNOERC165_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct DiamondLoupeFacetNoERC165<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for DiamondLoupeFacetNoERC165<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for DiamondLoupeFacetNoERC165<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for DiamondLoupeFacetNoERC165<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for DiamondLoupeFacetNoERC165<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(DiamondLoupeFacetNoERC165))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> DiamondLoupeFacetNoERC165<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                DIAMONDLOUPEFACETNOERC165_ABI.clone(),
                client,
            ))
        }
        /// Constructs the general purpose `Deployer` instance based on the provided constructor arguments and sends it.
        /// Returns a new instance of a deployer that returns an instance of this contract after sending the transaction
        ///
        /// Notes:
        /// - If there are no constructor arguments, you should pass `()` as the argument.
        /// - The default poll duration is 7 seconds.
        /// - The default number of confirmations is 1 block.
        ///
        ///
        /// # Example
        ///
        /// Generate contract bindings with `abigen!` and deploy a new contract instance.
        ///
        /// *Note*: this requires a `bytecode` and `abi` object in the `greeter.json` artifact.
        ///
        /// ```ignore
        /// # async fn deploy<M: ethers::providers::Middleware>(client: ::std::sync::Arc<M>) {
        ///     abigen!(Greeter, "../greeter.json");
        ///
        ///    let greeter_contract = Greeter::deploy(client, "Hello world!".to_string()).unwrap().send().await.unwrap();
        ///    let msg = greeter_contract.greet().call().await.unwrap();
        /// # }
        /// ```
        pub fn deploy<T: ::ethers::core::abi::Tokenize>(
            client: ::std::sync::Arc<M>,
            constructor_args: T,
        ) -> ::core::result::Result<
            ::ethers::contract::builders::ContractDeployer<M, Self>,
            ::ethers::contract::ContractError<M>,
        > {
            let factory = ::ethers::contract::ContractFactory::new(
                DIAMONDLOUPEFACETNOERC165_ABI.clone(),
                DIAMONDLOUPEFACETNOERC165_BYTECODE.clone(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `facetAddress` (0xcdffacc6) function
        pub fn facet_address(
            &self,
            function_selector: [u8; 4],
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([205, 255, 172, 198], function_selector)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `facetAddresses` (0x52ef6b2c) function
        pub fn facet_addresses(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<
            M,
            ::std::vec::Vec<::ethers::core::types::Address>,
        > {
            self.0
                .method_hash([82, 239, 107, 44], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `facetFunctionSelectors` (0xadfca15e) function
        pub fn facet_function_selectors(
            &self,
            facet: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<[u8; 4]>> {
            self.0
                .method_hash([173, 252, 161, 94], facet)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `facets` (0x7a0ed627) function
        pub fn facets(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<Facet>> {
            self.0
                .method_hash([122, 14, 214, 39], ())
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for DiamondLoupeFacetNoERC165<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Container type for all input parameters for the `facetAddress` function with signature `facetAddress(bytes4)` and selector `0xcdffacc6`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "facetAddress", abi = "facetAddress(bytes4)")]
    pub struct FacetAddressCall {
        pub function_selector: [u8; 4],
    }
    ///Container type for all input parameters for the `facetAddresses` function with signature `facetAddresses()` and selector `0x52ef6b2c`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "facetAddresses", abi = "facetAddresses()")]
    pub struct FacetAddressesCall;
    ///Container type for all input parameters for the `facetFunctionSelectors` function with signature `facetFunctionSelectors(address)` and selector `0xadfca15e`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(
        name = "facetFunctionSelectors",
        abi = "facetFunctionSelectors(address)"
    )]
    pub struct FacetFunctionSelectorsCall {
        pub facet: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `facets` function with signature `facets()` and selector `0x7a0ed627`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "facets", abi = "facets()")]
    pub struct FacetsCall;
    ///Container type for all of the contract's call
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        serde::Serialize,
        serde::Deserialize,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub enum DiamondLoupeFacetNoERC165Calls {
        FacetAddress(FacetAddressCall),
        FacetAddresses(FacetAddressesCall),
        FacetFunctionSelectors(FacetFunctionSelectorsCall),
        Facets(FacetsCall),
    }
    impl ::ethers::core::abi::AbiDecode for DiamondLoupeFacetNoERC165Calls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <FacetAddressCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::FacetAddress(decoded));
            }
            if let Ok(decoded) =
                <FacetAddressesCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::FacetAddresses(decoded));
            }
            if let Ok(decoded) =
                <FacetFunctionSelectorsCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::FacetFunctionSelectors(decoded));
            }
            if let Ok(decoded) = <FacetsCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::Facets(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for DiamondLoupeFacetNoERC165Calls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::FacetAddress(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::FacetAddresses(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::FacetFunctionSelectors(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::Facets(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for DiamondLoupeFacetNoERC165Calls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::FacetAddress(element) => ::core::fmt::Display::fmt(element, f),
                Self::FacetAddresses(element) => ::core::fmt::Display::fmt(element, f),
                Self::FacetFunctionSelectors(element) => ::core::fmt::Display::fmt(element, f),
                Self::Facets(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<FacetAddressCall> for DiamondLoupeFacetNoERC165Calls {
        fn from(value: FacetAddressCall) -> Self {
            Self::FacetAddress(value)
        }
    }
    impl ::core::convert::From<FacetAddressesCall> for DiamondLoupeFacetNoERC165Calls {
        fn from(value: FacetAddressesCall) -> Self {
            Self::FacetAddresses(value)
        }
    }
    impl ::core::convert::From<FacetFunctionSelectorsCall> for DiamondLoupeFacetNoERC165Calls {
        fn from(value: FacetFunctionSelectorsCall) -> Self {
            Self::FacetFunctionSelectors(value)
        }
    }
    impl ::core::convert::From<FacetsCall> for DiamondLoupeFacetNoERC165Calls {
        fn from(value: FacetsCall) -> Self {
            Self::Facets(value)
        }
    }
    ///Container type for all return fields from the `facetAddress` function with signature `facetAddress(bytes4)` and selector `0xcdffacc6`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct FacetAddressReturn {
        pub facet_address: ::ethers::core::types::Address,
    }
    ///Container type for all return fields from the `facetAddresses` function with signature `facetAddresses()` and selector `0x52ef6b2c`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct FacetAddressesReturn {
        pub facet_addresses: ::std::vec::Vec<::ethers::core::types::Address>,
    }
    ///Container type for all return fields from the `facetFunctionSelectors` function with signature `facetFunctionSelectors(address)` and selector `0xadfca15e`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct FacetFunctionSelectorsReturn {
        pub facet_function_selectors: ::std::vec::Vec<[u8; 4]>,
    }
    ///Container type for all return fields from the `facets` function with signature `facets()` and selector `0x7a0ed627`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct FacetsReturn {
        pub facets: ::std::vec::Vec<Facet>,
    }
    ///`Facet(address,bytes4[])`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct Facet {
        pub facet_address: ::ethers::core::types::Address,
        pub function_selectors: ::std::vec::Vec<[u8; 4]>,
    }
}
