pub use diamond_cut_facet::*;
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
pub mod diamond_cut_facet {
    const _: () = {
        ::core::include_bytes!("./DiamondCutFacet.json",);
    };
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("diamondCut"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("diamondCut"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_diamondCut"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(8usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Array(
                                                        ::std::boxed::Box::new(
                                                            ::ethers::core::abi::ethabi::ParamType::FixedBytes(4usize),
                                                        ),
                                                    ),
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IDiamond.FacetCut[]",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_init"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_calldata"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
            ]),
            events: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("DiamondCut"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("DiamondCut"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("_diamondCut"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(8usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Array(
                                                        ::std::boxed::Box::new(
                                                            ::ethers::core::abi::ethabi::ParamType::FixedBytes(4usize),
                                                        ),
                                                    ),
                                                ],
                                            ),
                                        ),
                                    ),
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("_init"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("_calldata"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("DiamondCut"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("_diamondCut"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(8usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Array(
                                                        ::std::boxed::Box::new(
                                                            ::ethers::core::abi::ethabi::ParamType::FixedBytes(4usize),
                                                        ),
                                                    ),
                                                ],
                                            ),
                                        ),
                                    ),
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("_init"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("_calldata"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
            ]),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned(
                        "CannotAddFunctionToDiamondThatAlreadyExists",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotAddFunctionToDiamondThatAlreadyExists",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selector"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        4usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("CannotAddSelectorsToZeroAddress"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotAddSelectorsToZeroAddress",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selectors"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::FixedBytes(4usize),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4[]"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned(
                        "CannotRemoveFunctionThatDoesNotExist",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotRemoveFunctionThatDoesNotExist",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selector"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        4usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("CannotRemoveImmutableFunction"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotRemoveImmutableFunction",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selector"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        4usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned(
                        "CannotReplaceFunctionThatDoesNotExists",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotReplaceFunctionThatDoesNotExists",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selector"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        4usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned(
                        "CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selector"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        4usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned(
                        "CannotReplaceFunctionsFromFacetWithZeroAddress",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotReplaceFunctionsFromFacetWithZeroAddress",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selectors"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::FixedBytes(4usize),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4[]"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("CannotReplaceImmutableFunction"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "CannotReplaceImmutableFunction",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_selector"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        4usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes4"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("IncorrectFacetCutAction"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "IncorrectFacetCutAction",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_action"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(8usize),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint8"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("InitializationFunctionReverted"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "InitializationFunctionReverted",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned(
                                        "_initializationContractAddress",
                                    ),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_calldata"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("NoBytecodeAtAddress"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "NoBytecodeAtAddress",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_contractAddress"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_message"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::String,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("string"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned(
                        "NoSelectorsProvidedForFacetForCut",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "NoSelectorsProvidedForFacetForCut",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_facetAddress"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("NotContractOwner"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("NotContractOwner"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_user"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_contractOwner"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned(
                        "RemoveFacetAddressMustBeZeroAddress",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "RemoveFacetAddressMustBeZeroAddress",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_facetAddress"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static DIAMONDCUTFACET_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80\x80`@R4`\x15Wa\x0C\xE3\x90\x81a\0\x1B\x829\xF3[`\0\x80\xFD\xFE`\x80`@R`\x046\x10\x15a\0\x12W`\0\x80\xFD[`\x005`\xE0\x1Cc\x1F\x93\x1C\x1C\x14a\0'W`\0\x80\xFD[4a\t\xB5W``6`\x03\x19\x01\x12a\t\xB5W`\x045`\x01`\x01`@\x1B\x03\x81\x11a\t\xB5W6`#\x82\x01\x12\x15a\t\xB5W\x80`\x04\x015\x90`\x01`\x01`@\x1B\x03\x82\x11a\t\xB5W`$\x82`\x05\x1B\x82\x01\x016\x81\x11a\t\xB5W`$5`\x01`\x01`\xA0\x1B\x03\x81\x16\x91\x90\x82\x81\x03a\t\xB5W`D5\x91`\x01`\x01`@\x1B\x03\x83\x11a\t\xB5W6`#\x84\x01\x12\x15a\t\xB5W\x82`\x04\x015`\x01`\x01`@\x1B\x03\x81\x11a\t\xB5W6`$\x82\x86\x01\x01\x11a\t\xB5W\x7F\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1FT`\x01`\x01`\xA0\x1B\x03\x163\x81\x90\x03a\t\x9DWP` a\x01\x16a\x01\x11\x89\x98\x97\x95\x94\x99a\n\x14V[a\t\xEFV[\x80\x97\x81R\x01\x96\x87`\0\x96`$\x81\x01\x91[\x83\x83\x10a\x08tWPPPP` \x81\x80`$a\x01Da\x01\x11\x8A\x96a\n+V[\x97\x82\x89R\x01\x83\x88\x017\x85\x01\x01R\x83\x94[\x80Q\x86\x10\x15a\x07SW`@a\x01i\x87\x83a\nFV[Q\x01Q\x92`\x01`\x01`\xA0\x1B\x03a\x01\x7F\x88\x84a\nFV[QQ\x16\x94\x84Q\x15a\x07?W` a\x01\x96\x89\x85a\nFV[Q\x01Q`\x03\x81\x10\x15a\x07+W\x80a\x03\xA9WP\x85\x15a\x03\x8AWa\xFF\xFF`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x16\x93a\x02\ra\x01\xD0``a\t\xEFV[`$\x81R\x7FLibDiamondCut: Add facet has no ` \x82\x01Rccode`\xE0\x1B`@\x82\x01R\x88a\x0C.V[\x87\x94[\x86Q\x86\x10\x15a\x03qW`\x01`\x01`\xE0\x1B\x03\x19a\x02,\x87\x89a\nFV[Q\x16\x80\x8AR`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x8A T\x90\x91\x90`\x01`\x01`\xA0\x1B\x03\x16a\x03]W\x90a\x02\xC6\x8A\x92a\xFF\xFFa\x02ia\t\xBAV[\x8C\x81R\x91\x81\x16` \x80\x84\x01\x82\x81R\x86\x88R`\0\x80Q` a\x0Cn\x839\x81Q\x91R\x90\x91R`@\x90\x96 \x92Q\x83T\x96Q`\x01`\x01`\xB0\x1B\x03\x19\x90\x97\x16`\x01`\x01`\xA0\x1B\x03\x91\x90\x91\x16\x17\x95\x90\x91\x16`\xA0\x1Ba\xFF\xFF`\xA0\x1B\x16\x94\x90\x94\x17\x90UV[`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT`\x01`@\x1B\x81\x10\x15a\x03IW\x90a\x03\x03\x82`\x01a\x03 \x94\x01`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RUa\n\xB1V[\x90\x91\x90c\xFF\xFF\xFF\xFF\x83T\x91`\x03\x1B\x92`\xE0\x1C\x83\x1B\x92\x1B\x19\x16\x17\x90UV[a\xFF\xFF\x81\x14a\x035W`\x01\x95\x86\x01\x95\x01a\x02\x10V[cNH{q`\xE0\x1B\x89R`\x11`\x04R`$\x89\xFD[cNH{q`\xE0\x1B\x8BR`A`\x04R`$\x8B\xFD[c\xEB\xBF]\x07`\xE0\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[P\x94P\x94P\x94\x95`\x01\x91\x97\x92P[\x01\x94\x93\x91\x90\x95a\x01TV[`@Qc\x02\xB8\xDA\x07`\xE2\x1B\x81R\x80a\x03\xA5\x87`\x04\x83\x01a\x0B\x0CV[\x03\x90\xFD[\x95\x98\x96\x95`\x01\x81\x03a\x05\x05WP\x88\x15a\x04\xEAW\x92\x95\x92a\x04\ra\x03\xCC``a\t\xEFV[`(\x81R\x7FLibDiamondCut: Replace facet has` \x82\x01Rg no code`\xC0\x1B`@\x82\x01R\x8Aa\x0C.V[`\x01`\x01`\xA0\x1B\x03\x89\x16\x96\x86[\x86Q\x81\x10\x15a\x04\xD8W`\x01`\x01`\xE0\x1B\x03\x19a\x046\x82\x89a\nFV[Q\x16\x80\x89R`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x89 T`\x01`\x01`\xA0\x1B\x03\x160\x81\x14a\x04\xC4W\x8C\x81\x14a\x04\xB0W\x15a\x04\x9EW\x88R`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x88 \x80T`\x01`\x01`\xA0\x1B\x03\x19\x16\x8A\x17\x90U`\x01\x01a\x04\x1AV[cty\xF99`\xE0\x1B\x89R`\x04R`$\x88\xFD[c\x1A\xC6\xCE\x8D`\xE1\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[c)\x01\x80m`\xE1\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[P\x94P\x94\x96\x90\x95P`\x01\x91\x97Pa\x03\x7FV[`@Qc\xCD\x98\xA9o`\xE0\x1B\x81R\x80a\x03\xA5\x87`\x04\x83\x01a\x0B\x0CV[\x86\x97\x95\x96\x94\x91\x92\x93\x94P`\x02\x81\x14`\0\x14a\x07\x16WP`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x98\x80a\x07\x04WP\x86[\x86Q\x81\x10\x15a\x06\xF3Wa\xFF\xFF\x99`\x01`\x01`\xE0\x1B\x03\x19a\x05T\x83\x8Aa\nFV[Q\x16\x90\x81\x8AR`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x8A a\x05xa\t\xBAV[\x90T`\x01`\x01`\xA0\x1B\x03\x81\x16\x80\x83R`\xA0\x91\x90\x91\x1C\x90\x9D\x16` \x82\x01\x90\x81R\x9C\x15a\x06\xDFWQ`\x01`\x01`\xA0\x1B\x03\x160\x14a\x06\xCBW\x80\x15a\x06\xB7W`\0\x19\x01\x8B\x81a\xFF\xFF\x81\x9EQ\x16\x03a\x06DW[PP`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x80\x15a\x060W`\x01\x92\x91\x90`\0\x19\x01a\x05\xF0\x81a\n\xB1V[c\xFF\xFF\xFF\xFF\x82T\x91`\x03\x1B\x1B\x19\x16\x90U`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RU\x89R`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R\x88`@\x81 U\x01a\x054V[cNH{q`\xE0\x1B\x8AR`1`\x04R`$\x8A\xFD[a\xFF\xFFa\x06Sa\x06\xB0\x93a\n\xB1V[\x90T\x90`\x03\x1B\x1C`\xE0\x1B\x91a\x06n\x83a\x03\x03\x84\x84Q\x16a\n\xB1V[Q`\x01`\x01`\xE0\x1B\x03\x19\x90\x92\x16\x8CR`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x8C \x80Ta\xFF\xFF`\xA0\x1B\x19\x16\x91\x90\x92\x16`\xA0\x1Ba\xFF\xFF`\xA0\x1B\x16\x17\x90UV[\x8A8a\x05\xC6V[cNH{q`\xE0\x1B\x8AR`\x11`\x04R`$\x8A\xFD[c\r\xF5\xFDa`\xE3\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[cz\x08\xA2-`\xE0\x1B\x8BR`\x04\x83\x90R`$\x8B\xFD[P\x93\x92\x97P\x93P\x93\x94`\x01\x90a\x03\x7FV[c\xD0\x91\xBC\x81`\xE0\x1B\x88R`\x04R`$\x87\xFD[c?\xF4\xD2\x0F`\xE1\x1B\x88R`\xFF\x16`\x04R`$\x87\xFD[cNH{q`\xE0\x1B\x88R`!`\x04R`$\x88\xFD[c\xE7g\xF9\x1F`\xE0\x1B\x87R`\x04\x86\x90R`$\x87\xFD[\x92\x91\x86\x90`@Q\x94``\x86\x01\x90``\x87RQ\x80\x91R`\x80\x86\x01\x90`\x80\x81`\x05\x1B\x88\x01\x01\x93\x91\x88\x90[\x82\x82\x10a\x07\xCFWPPPP\x93\x80a\x07\xC4\x7F\x8F\xAAp\x87\x86q\xCC\xD2\x12\xD2\x07q\xB7\x95\xC5\n\xF8\xFD?\xF6\xCF'\xF4\xBD\xE5~]M\xE0\xAE\xB6s\x93a\x07\xCC\x97` \x84\x01R\x82\x81\x03`@\x84\x01R\x86a\npV[\x03\x90\xA1a\x0BPV[\x80\xF3[\x90\x91\x92\x94`\x7F\x19\x89\x82\x03\x01\x82R\x85Q``\x82\x01\x90`\x01\x80`\xA0\x1B\x03\x81Q\x16\x83R` \x81\x01Q`\x03\x81\x10\x15a\x08`W`@` \x92`\x80\x92\x84\x87\x01R\x01Q\x93```@\x82\x01R\x84Q\x80\x94R\x01\x92\x01\x90\x8B\x90[\x80\x82\x10a\x08=WPPP` \x80`\x01\x92\x97\x01\x92\x01\x92\x01\x90\x92\x91a\x07{V[\x82Q`\x01`\x01`\xE0\x1B\x03\x19\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x90\x91\x01\x90a\x08\x1FV[cNH{q`\xE0\x1B\x8DR`!`\x04R`$\x8D\xFD[\x82\x9A\x95\x96\x98\x99\x9A5`\x01`\x01`@\x1B\x03\x81\x11a\t\x99W\x82\x01```#\x19\x826\x03\x01\x12a\t\x99W`@Q\x90``\x82\x01\x82\x81\x10`\x01`\x01`@\x1B\x03\x82\x11\x17a\t\x85W`@R`$\x81\x015`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x03a\t\x81W\x82R`D\x81\x015`\x03\x81\x10\x15a\t\x81W` \x83\x01R`d\x81\x015\x90`\x01`\x01`@\x1B\x03\x82\x11a\t\x81W\x90`$\x91\x01\x016`\x1F\x82\x01\x12\x15a\t}W\x805a\t\x14a\x01\x11\x82a\n\x14V[\x91` \x80\x84\x84\x81R\x01\x92`\x05\x1B\x82\x01\x01\x906\x82\x11a\tyW` \x01\x91[\x81\x83\x10a\tTWPPP`@\x82\x01R\x81R\x94\x99\x98\x97\x95\x94` \x92\x83\x01\x92\x01a\x01&V[\x825`\x01`\x01`\xE0\x1B\x03\x19\x81\x16\x81\x03a\tuW\x81R` \x92\x83\x01\x92\x01a\t1V[\x8F\x80\xFD[\x8E\x80\xFD[\x8B\x80\xFD[\x8C\x80\xFD[cNH{q`\xE0\x1B\x8DR`A`\x04R`$\x8D\xFD[\x8A\x80\xFD[c\xFFA'\xCB`\xE0\x1B`\0R3`\x04R`$R`D`\0\xFD[`\0\x80\xFD[`@Q\x90`@\x82\x01\x82\x81\x10`\x01`\x01`@\x1B\x03\x82\x11\x17a\t\xD9W`@RV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`@Q\x91\x90`\x1F\x01`\x1F\x19\x16\x82\x01`\x01`\x01`@\x1B\x03\x81\x11\x83\x82\x10\x17a\t\xD9W`@RV[`\x01`\x01`@\x1B\x03\x81\x11a\t\xD9W`\x05\x1B` \x01\x90V[`\x01`\x01`@\x1B\x03\x81\x11a\t\xD9W`\x1F\x01`\x1F\x19\x16` \x01\x90V[\x80Q\x82\x10\x15a\nZW` \x91`\x05\x1B\x01\x01\x90V[cNH{q`\xE0\x1B`\0R`2`\x04R`$`\0\xFD[\x91\x90\x82Q\x92\x83\x82R`\0[\x84\x81\x10a\n\x9CWPP\x82`\0` \x80\x94\x95\x84\x01\x01R`\x1F\x80\x19\x91\x01\x16\x01\x01\x90V[\x80` \x80\x92\x84\x01\x01Q\x82\x82\x86\x01\x01R\x01a\n{V[\x90`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x82\x10\x15a\nZW`\0\x80Q` a\x0C\x8E\x839\x81Q\x91R`\0R`\x03\x82\x90\x1C\x7F\xC0\xD7'a\x0E\xA1bA\xEF\xF4D}\x08\xBB\x1BE\x95\xF7\xD2\xECE\x15($7\xA1;}\r\xF4\xB9\"\x01\x91`\x02\x1B`\x1C\x16\x90V[` `@\x81\x83\x01\x92\x82\x81R\x84Q\x80\x94R\x01\x92\x01\x90`\0[\x81\x81\x10a\x0B0WPPP\x90V[\x82Q`\x01`\x01`\xE0\x1B\x03\x19\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x01a\x0B#V[`\x01`\x01`\xA0\x1B\x03\x81\x16\x91\x90\x82\x15a\x0C)W`\0\x80\x91a\x0B\xB4a\x0Bs``a\t\xEFV[`(\x81R\x7FLibDiamondCut: _init address has` \x82\x01Rg no code`\xC0\x1B`@\x82\x01R\x82a\x0C.V[\x83Q\x90` \x85\x01\x90Z\xF4\x91=\x15a\x0C!W=\x92a\x0B\xD3a\x01\x11\x85a\n+V[\x93\x84R=`\0` \x86\x01>[\x15a\x0B\xE9WPPPV[\x82Q\x15a\x0B\xF8W\x82Q` \x84\x01\xFD[a\x03\xA5`@Q\x92\x83\x92c\x19!\x05\xD7`\xE0\x1B\x84R`\x04\x84\x01R`@`$\x84\x01R`D\x83\x01\x90a\npV[``\x92a\x0B\xDFV[PPPV[\x80;\x15a\x0C9WPPV[`@\x80Qc\x91\x984\xB9`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x92\x16`\x04\x83\x01R`$\x82\x01R\x90\x81\x90a\x03\xA5\x90`D\x83\x01\x90a\npV\xFE\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1C\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1D\xA2dipfsX\"\x12 \xC2\xC5ks6\x80PE\x167rJ\x99Xbn\xE4\xD8\x1F\x94\x03\xEC\xE2\xF7\x13\xF4\x04\x14\0(s\x02dsolcC\0\x08\x1C\x003";
    /// The bytecode of the contract.
    pub static DIAMONDCUTFACET_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\x046\x10\x15a\0\x12W`\0\x80\xFD[`\x005`\xE0\x1Cc\x1F\x93\x1C\x1C\x14a\0'W`\0\x80\xFD[4a\t\xB5W``6`\x03\x19\x01\x12a\t\xB5W`\x045`\x01`\x01`@\x1B\x03\x81\x11a\t\xB5W6`#\x82\x01\x12\x15a\t\xB5W\x80`\x04\x015\x90`\x01`\x01`@\x1B\x03\x82\x11a\t\xB5W`$\x82`\x05\x1B\x82\x01\x016\x81\x11a\t\xB5W`$5`\x01`\x01`\xA0\x1B\x03\x81\x16\x91\x90\x82\x81\x03a\t\xB5W`D5\x91`\x01`\x01`@\x1B\x03\x83\x11a\t\xB5W6`#\x84\x01\x12\x15a\t\xB5W\x82`\x04\x015`\x01`\x01`@\x1B\x03\x81\x11a\t\xB5W6`$\x82\x86\x01\x01\x11a\t\xB5W\x7F\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1FT`\x01`\x01`\xA0\x1B\x03\x163\x81\x90\x03a\t\x9DWP` a\x01\x16a\x01\x11\x89\x98\x97\x95\x94\x99a\n\x14V[a\t\xEFV[\x80\x97\x81R\x01\x96\x87`\0\x96`$\x81\x01\x91[\x83\x83\x10a\x08tWPPPP` \x81\x80`$a\x01Da\x01\x11\x8A\x96a\n+V[\x97\x82\x89R\x01\x83\x88\x017\x85\x01\x01R\x83\x94[\x80Q\x86\x10\x15a\x07SW`@a\x01i\x87\x83a\nFV[Q\x01Q\x92`\x01`\x01`\xA0\x1B\x03a\x01\x7F\x88\x84a\nFV[QQ\x16\x94\x84Q\x15a\x07?W` a\x01\x96\x89\x85a\nFV[Q\x01Q`\x03\x81\x10\x15a\x07+W\x80a\x03\xA9WP\x85\x15a\x03\x8AWa\xFF\xFF`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x16\x93a\x02\ra\x01\xD0``a\t\xEFV[`$\x81R\x7FLibDiamondCut: Add facet has no ` \x82\x01Rccode`\xE0\x1B`@\x82\x01R\x88a\x0C.V[\x87\x94[\x86Q\x86\x10\x15a\x03qW`\x01`\x01`\xE0\x1B\x03\x19a\x02,\x87\x89a\nFV[Q\x16\x80\x8AR`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x8A T\x90\x91\x90`\x01`\x01`\xA0\x1B\x03\x16a\x03]W\x90a\x02\xC6\x8A\x92a\xFF\xFFa\x02ia\t\xBAV[\x8C\x81R\x91\x81\x16` \x80\x84\x01\x82\x81R\x86\x88R`\0\x80Q` a\x0Cn\x839\x81Q\x91R\x90\x91R`@\x90\x96 \x92Q\x83T\x96Q`\x01`\x01`\xB0\x1B\x03\x19\x90\x97\x16`\x01`\x01`\xA0\x1B\x03\x91\x90\x91\x16\x17\x95\x90\x91\x16`\xA0\x1Ba\xFF\xFF`\xA0\x1B\x16\x94\x90\x94\x17\x90UV[`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT`\x01`@\x1B\x81\x10\x15a\x03IW\x90a\x03\x03\x82`\x01a\x03 \x94\x01`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RUa\n\xB1V[\x90\x91\x90c\xFF\xFF\xFF\xFF\x83T\x91`\x03\x1B\x92`\xE0\x1C\x83\x1B\x92\x1B\x19\x16\x17\x90UV[a\xFF\xFF\x81\x14a\x035W`\x01\x95\x86\x01\x95\x01a\x02\x10V[cNH{q`\xE0\x1B\x89R`\x11`\x04R`$\x89\xFD[cNH{q`\xE0\x1B\x8BR`A`\x04R`$\x8B\xFD[c\xEB\xBF]\x07`\xE0\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[P\x94P\x94P\x94\x95`\x01\x91\x97\x92P[\x01\x94\x93\x91\x90\x95a\x01TV[`@Qc\x02\xB8\xDA\x07`\xE2\x1B\x81R\x80a\x03\xA5\x87`\x04\x83\x01a\x0B\x0CV[\x03\x90\xFD[\x95\x98\x96\x95`\x01\x81\x03a\x05\x05WP\x88\x15a\x04\xEAW\x92\x95\x92a\x04\ra\x03\xCC``a\t\xEFV[`(\x81R\x7FLibDiamondCut: Replace facet has` \x82\x01Rg no code`\xC0\x1B`@\x82\x01R\x8Aa\x0C.V[`\x01`\x01`\xA0\x1B\x03\x89\x16\x96\x86[\x86Q\x81\x10\x15a\x04\xD8W`\x01`\x01`\xE0\x1B\x03\x19a\x046\x82\x89a\nFV[Q\x16\x80\x89R`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x89 T`\x01`\x01`\xA0\x1B\x03\x160\x81\x14a\x04\xC4W\x8C\x81\x14a\x04\xB0W\x15a\x04\x9EW\x88R`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x88 \x80T`\x01`\x01`\xA0\x1B\x03\x19\x16\x8A\x17\x90U`\x01\x01a\x04\x1AV[cty\xF99`\xE0\x1B\x89R`\x04R`$\x88\xFD[c\x1A\xC6\xCE\x8D`\xE1\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[c)\x01\x80m`\xE1\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[P\x94P\x94\x96\x90\x95P`\x01\x91\x97Pa\x03\x7FV[`@Qc\xCD\x98\xA9o`\xE0\x1B\x81R\x80a\x03\xA5\x87`\x04\x83\x01a\x0B\x0CV[\x86\x97\x95\x96\x94\x91\x92\x93\x94P`\x02\x81\x14`\0\x14a\x07\x16WP`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x98\x80a\x07\x04WP\x86[\x86Q\x81\x10\x15a\x06\xF3Wa\xFF\xFF\x99`\x01`\x01`\xE0\x1B\x03\x19a\x05T\x83\x8Aa\nFV[Q\x16\x90\x81\x8AR`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x8A a\x05xa\t\xBAV[\x90T`\x01`\x01`\xA0\x1B\x03\x81\x16\x80\x83R`\xA0\x91\x90\x91\x1C\x90\x9D\x16` \x82\x01\x90\x81R\x9C\x15a\x06\xDFWQ`\x01`\x01`\xA0\x1B\x03\x160\x14a\x06\xCBW\x80\x15a\x06\xB7W`\0\x19\x01\x8B\x81a\xFF\xFF\x81\x9EQ\x16\x03a\x06DW[PP`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x80\x15a\x060W`\x01\x92\x91\x90`\0\x19\x01a\x05\xF0\x81a\n\xB1V[c\xFF\xFF\xFF\xFF\x82T\x91`\x03\x1B\x1B\x19\x16\x90U`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RU\x89R`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R\x88`@\x81 U\x01a\x054V[cNH{q`\xE0\x1B\x8AR`1`\x04R`$\x8A\xFD[a\xFF\xFFa\x06Sa\x06\xB0\x93a\n\xB1V[\x90T\x90`\x03\x1B\x1C`\xE0\x1B\x91a\x06n\x83a\x03\x03\x84\x84Q\x16a\n\xB1V[Q`\x01`\x01`\xE0\x1B\x03\x19\x90\x92\x16\x8CR`\0\x80Q` a\x0Cn\x839\x81Q\x91R` R`@\x8C \x80Ta\xFF\xFF`\xA0\x1B\x19\x16\x91\x90\x92\x16`\xA0\x1Ba\xFF\xFF`\xA0\x1B\x16\x17\x90UV[\x8A8a\x05\xC6V[cNH{q`\xE0\x1B\x8AR`\x11`\x04R`$\x8A\xFD[c\r\xF5\xFDa`\xE3\x1B\x8AR`\x04\x82\x90R`$\x8A\xFD[cz\x08\xA2-`\xE0\x1B\x8BR`\x04\x83\x90R`$\x8B\xFD[P\x93\x92\x97P\x93P\x93\x94`\x01\x90a\x03\x7FV[c\xD0\x91\xBC\x81`\xE0\x1B\x88R`\x04R`$\x87\xFD[c?\xF4\xD2\x0F`\xE1\x1B\x88R`\xFF\x16`\x04R`$\x87\xFD[cNH{q`\xE0\x1B\x88R`!`\x04R`$\x88\xFD[c\xE7g\xF9\x1F`\xE0\x1B\x87R`\x04\x86\x90R`$\x87\xFD[\x92\x91\x86\x90`@Q\x94``\x86\x01\x90``\x87RQ\x80\x91R`\x80\x86\x01\x90`\x80\x81`\x05\x1B\x88\x01\x01\x93\x91\x88\x90[\x82\x82\x10a\x07\xCFWPPPP\x93\x80a\x07\xC4\x7F\x8F\xAAp\x87\x86q\xCC\xD2\x12\xD2\x07q\xB7\x95\xC5\n\xF8\xFD?\xF6\xCF'\xF4\xBD\xE5~]M\xE0\xAE\xB6s\x93a\x07\xCC\x97` \x84\x01R\x82\x81\x03`@\x84\x01R\x86a\npV[\x03\x90\xA1a\x0BPV[\x80\xF3[\x90\x91\x92\x94`\x7F\x19\x89\x82\x03\x01\x82R\x85Q``\x82\x01\x90`\x01\x80`\xA0\x1B\x03\x81Q\x16\x83R` \x81\x01Q`\x03\x81\x10\x15a\x08`W`@` \x92`\x80\x92\x84\x87\x01R\x01Q\x93```@\x82\x01R\x84Q\x80\x94R\x01\x92\x01\x90\x8B\x90[\x80\x82\x10a\x08=WPPP` \x80`\x01\x92\x97\x01\x92\x01\x92\x01\x90\x92\x91a\x07{V[\x82Q`\x01`\x01`\xE0\x1B\x03\x19\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x90\x91\x01\x90a\x08\x1FV[cNH{q`\xE0\x1B\x8DR`!`\x04R`$\x8D\xFD[\x82\x9A\x95\x96\x98\x99\x9A5`\x01`\x01`@\x1B\x03\x81\x11a\t\x99W\x82\x01```#\x19\x826\x03\x01\x12a\t\x99W`@Q\x90``\x82\x01\x82\x81\x10`\x01`\x01`@\x1B\x03\x82\x11\x17a\t\x85W`@R`$\x81\x015`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x03a\t\x81W\x82R`D\x81\x015`\x03\x81\x10\x15a\t\x81W` \x83\x01R`d\x81\x015\x90`\x01`\x01`@\x1B\x03\x82\x11a\t\x81W\x90`$\x91\x01\x016`\x1F\x82\x01\x12\x15a\t}W\x805a\t\x14a\x01\x11\x82a\n\x14V[\x91` \x80\x84\x84\x81R\x01\x92`\x05\x1B\x82\x01\x01\x906\x82\x11a\tyW` \x01\x91[\x81\x83\x10a\tTWPPP`@\x82\x01R\x81R\x94\x99\x98\x97\x95\x94` \x92\x83\x01\x92\x01a\x01&V[\x825`\x01`\x01`\xE0\x1B\x03\x19\x81\x16\x81\x03a\tuW\x81R` \x92\x83\x01\x92\x01a\t1V[\x8F\x80\xFD[\x8E\x80\xFD[\x8B\x80\xFD[\x8C\x80\xFD[cNH{q`\xE0\x1B\x8DR`A`\x04R`$\x8D\xFD[\x8A\x80\xFD[c\xFFA'\xCB`\xE0\x1B`\0R3`\x04R`$R`D`\0\xFD[`\0\x80\xFD[`@Q\x90`@\x82\x01\x82\x81\x10`\x01`\x01`@\x1B\x03\x82\x11\x17a\t\xD9W`@RV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`@Q\x91\x90`\x1F\x01`\x1F\x19\x16\x82\x01`\x01`\x01`@\x1B\x03\x81\x11\x83\x82\x10\x17a\t\xD9W`@RV[`\x01`\x01`@\x1B\x03\x81\x11a\t\xD9W`\x05\x1B` \x01\x90V[`\x01`\x01`@\x1B\x03\x81\x11a\t\xD9W`\x1F\x01`\x1F\x19\x16` \x01\x90V[\x80Q\x82\x10\x15a\nZW` \x91`\x05\x1B\x01\x01\x90V[cNH{q`\xE0\x1B`\0R`2`\x04R`$`\0\xFD[\x91\x90\x82Q\x92\x83\x82R`\0[\x84\x81\x10a\n\x9CWPP\x82`\0` \x80\x94\x95\x84\x01\x01R`\x1F\x80\x19\x91\x01\x16\x01\x01\x90V[\x80` \x80\x92\x84\x01\x01Q\x82\x82\x86\x01\x01R\x01a\n{V[\x90`\0\x80Q` a\x0C\x8E\x839\x81Q\x91RT\x82\x10\x15a\nZW`\0\x80Q` a\x0C\x8E\x839\x81Q\x91R`\0R`\x03\x82\x90\x1C\x7F\xC0\xD7'a\x0E\xA1bA\xEF\xF4D}\x08\xBB\x1BE\x95\xF7\xD2\xECE\x15($7\xA1;}\r\xF4\xB9\"\x01\x91`\x02\x1B`\x1C\x16\x90V[` `@\x81\x83\x01\x92\x82\x81R\x84Q\x80\x94R\x01\x92\x01\x90`\0[\x81\x81\x10a\x0B0WPPP\x90V[\x82Q`\x01`\x01`\xE0\x1B\x03\x19\x16\x84R` \x93\x84\x01\x93\x90\x92\x01\x91`\x01\x01a\x0B#V[`\x01`\x01`\xA0\x1B\x03\x81\x16\x91\x90\x82\x15a\x0C)W`\0\x80\x91a\x0B\xB4a\x0Bs``a\t\xEFV[`(\x81R\x7FLibDiamondCut: _init address has` \x82\x01Rg no code`\xC0\x1B`@\x82\x01R\x82a\x0C.V[\x83Q\x90` \x85\x01\x90Z\xF4\x91=\x15a\x0C!W=\x92a\x0B\xD3a\x01\x11\x85a\n+V[\x93\x84R=`\0` \x86\x01>[\x15a\x0B\xE9WPPPV[\x82Q\x15a\x0B\xF8W\x82Q` \x84\x01\xFD[a\x03\xA5`@Q\x92\x83\x92c\x19!\x05\xD7`\xE0\x1B\x84R`\x04\x84\x01R`@`$\x84\x01R`D\x83\x01\x90a\npV[``\x92a\x0B\xDFV[PPPV[\x80;\x15a\x0C9WPPV[`@\x80Qc\x91\x984\xB9`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x90\x92\x16`\x04\x83\x01R`$\x82\x01R\x90\x81\x90a\x03\xA5\x90`D\x83\x01\x90a\npV\xFE\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1C\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1D\xA2dipfsX\"\x12 \xC2\xC5ks6\x80PE\x167rJ\x99Xbn\xE4\xD8\x1F\x94\x03\xEC\xE2\xF7\x13\xF4\x04\x14\0(s\x02dsolcC\0\x08\x1C\x003";
    /// The deployed bytecode of the contract.
    pub static DIAMONDCUTFACET_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct DiamondCutFacet<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for DiamondCutFacet<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for DiamondCutFacet<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for DiamondCutFacet<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for DiamondCutFacet<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(DiamondCutFacet))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> DiamondCutFacet<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                DIAMONDCUTFACET_ABI.clone(),
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
                DIAMONDCUTFACET_ABI.clone(),
                DIAMONDCUTFACET_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `diamondCut` (0x1f931c1c) function
        pub fn diamond_cut(
            &self,
            diamond_cut: ::std::vec::Vec<FacetCut>,
            init: ::ethers::core::types::Address,
            calldata: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([31, 147, 28, 28], (diamond_cut, init, calldata))
                .expect("method not found (this should never happen)")
        }
        ///Gets the contract's `DiamondCut` event
        pub fn diamond_cut_2_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, DiamondCut2Filter>
        {
            self.0.event()
        }
        /// Returns an `Event` builder for all the events of this contract.
        pub fn events(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, DiamondCutFacetEvents>
        {
            self.0
                .event_with_filter(::core::default::Default::default())
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for DiamondCutFacet<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `CannotAddFunctionToDiamondThatAlreadyExists` with signature `CannotAddFunctionToDiamondThatAlreadyExists(bytes4)` and selector `0xebbf5d07`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotAddFunctionToDiamondThatAlreadyExists",
        abi = "CannotAddFunctionToDiamondThatAlreadyExists(bytes4)"
    )]
    pub struct CannotAddFunctionToDiamondThatAlreadyExists {
        pub selector: [u8; 4],
    }
    ///Custom Error type `CannotAddSelectorsToZeroAddress` with signature `CannotAddSelectorsToZeroAddress(bytes4[])` and selector `0x0ae3681c`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotAddSelectorsToZeroAddress",
        abi = "CannotAddSelectorsToZeroAddress(bytes4[])"
    )]
    pub struct CannotAddSelectorsToZeroAddress {
        pub selectors: ::std::vec::Vec<[u8; 4]>,
    }
    ///Custom Error type `CannotRemoveFunctionThatDoesNotExist` with signature `CannotRemoveFunctionThatDoesNotExist(bytes4)` and selector `0x7a08a22d`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotRemoveFunctionThatDoesNotExist",
        abi = "CannotRemoveFunctionThatDoesNotExist(bytes4)"
    )]
    pub struct CannotRemoveFunctionThatDoesNotExist {
        pub selector: [u8; 4],
    }
    ///Custom Error type `CannotRemoveImmutableFunction` with signature `CannotRemoveImmutableFunction(bytes4)` and selector `0x6fafeb08`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotRemoveImmutableFunction",
        abi = "CannotRemoveImmutableFunction(bytes4)"
    )]
    pub struct CannotRemoveImmutableFunction {
        pub selector: [u8; 4],
    }
    ///Custom Error type `CannotReplaceFunctionThatDoesNotExists` with signature `CannotReplaceFunctionThatDoesNotExists(bytes4)` and selector `0x7479f939`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotReplaceFunctionThatDoesNotExists",
        abi = "CannotReplaceFunctionThatDoesNotExists(bytes4)"
    )]
    pub struct CannotReplaceFunctionThatDoesNotExists {
        pub selector: [u8; 4],
    }
    ///Custom Error type `CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet` with signature `CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet(bytes4)` and selector `0x358d9d1a`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet",
        abi = "CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet(bytes4)"
    )]
    pub struct CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet {
        pub selector: [u8; 4],
    }
    ///Custom Error type `CannotReplaceFunctionsFromFacetWithZeroAddress` with signature `CannotReplaceFunctionsFromFacetWithZeroAddress(bytes4[])` and selector `0xcd98a96f`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotReplaceFunctionsFromFacetWithZeroAddress",
        abi = "CannotReplaceFunctionsFromFacetWithZeroAddress(bytes4[])"
    )]
    pub struct CannotReplaceFunctionsFromFacetWithZeroAddress {
        pub selectors: ::std::vec::Vec<[u8; 4]>,
    }
    ///Custom Error type `CannotReplaceImmutableFunction` with signature `CannotReplaceImmutableFunction(bytes4)` and selector `0x520300da`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "CannotReplaceImmutableFunction",
        abi = "CannotReplaceImmutableFunction(bytes4)"
    )]
    pub struct CannotReplaceImmutableFunction {
        pub selector: [u8; 4],
    }
    ///Custom Error type `IncorrectFacetCutAction` with signature `IncorrectFacetCutAction(uint8)` and selector `0x7fe9a41e`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "IncorrectFacetCutAction",
        abi = "IncorrectFacetCutAction(uint8)"
    )]
    pub struct IncorrectFacetCutAction {
        pub action: u8,
    }
    ///Custom Error type `InitializationFunctionReverted` with signature `InitializationFunctionReverted(address,bytes)` and selector `0x192105d7`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "InitializationFunctionReverted",
        abi = "InitializationFunctionReverted(address,bytes)"
    )]
    pub struct InitializationFunctionReverted {
        pub initialization_contract_address: ::ethers::core::types::Address,
        pub calldata: ::ethers::core::types::Bytes,
    }
    ///Custom Error type `NoBytecodeAtAddress` with signature `NoBytecodeAtAddress(address,string)` and selector `0x919834b9`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "NoBytecodeAtAddress",
        abi = "NoBytecodeAtAddress(address,string)"
    )]
    pub struct NoBytecodeAtAddress {
        pub contract_address: ::ethers::core::types::Address,
        pub message: ::std::string::String,
    }
    ///Custom Error type `NoSelectorsProvidedForFacetForCut` with signature `NoSelectorsProvidedForFacetForCut(address)` and selector `0xe767f91f`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "NoSelectorsProvidedForFacetForCut",
        abi = "NoSelectorsProvidedForFacetForCut(address)"
    )]
    pub struct NoSelectorsProvidedForFacetForCut {
        pub facet_address: ::ethers::core::types::Address,
    }
    ///Custom Error type `NotContractOwner` with signature `NotContractOwner(address,address)` and selector `0xff4127cb`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(name = "NotContractOwner", abi = "NotContractOwner(address,address)")]
    pub struct NotContractOwner {
        pub user: ::ethers::core::types::Address,
        pub contract_owner: ::ethers::core::types::Address,
    }
    ///Custom Error type `RemoveFacetAddressMustBeZeroAddress` with signature `RemoveFacetAddressMustBeZeroAddress(address)` and selector `0xd091bc81`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "RemoveFacetAddressMustBeZeroAddress",
        abi = "RemoveFacetAddressMustBeZeroAddress(address)"
    )]
    pub struct RemoveFacetAddressMustBeZeroAddress {
        pub facet_address: ::ethers::core::types::Address,
    }
    ///Container type for all of the contract's custom errors
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
    pub enum DiamondCutFacetErrors {
        CannotAddFunctionToDiamondThatAlreadyExists(CannotAddFunctionToDiamondThatAlreadyExists),
        CannotAddSelectorsToZeroAddress(CannotAddSelectorsToZeroAddress),
        CannotRemoveFunctionThatDoesNotExist(CannotRemoveFunctionThatDoesNotExist),
        CannotRemoveImmutableFunction(CannotRemoveImmutableFunction),
        CannotReplaceFunctionThatDoesNotExists(CannotReplaceFunctionThatDoesNotExists),
        CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet(
            CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet,
        ),
        CannotReplaceFunctionsFromFacetWithZeroAddress(
            CannotReplaceFunctionsFromFacetWithZeroAddress,
        ),
        CannotReplaceImmutableFunction(CannotReplaceImmutableFunction),
        IncorrectFacetCutAction(IncorrectFacetCutAction),
        InitializationFunctionReverted(InitializationFunctionReverted),
        NoBytecodeAtAddress(NoBytecodeAtAddress),
        NoSelectorsProvidedForFacetForCut(NoSelectorsProvidedForFacetForCut),
        NotContractOwner(NotContractOwner),
        RemoveFacetAddressMustBeZeroAddress(RemoveFacetAddressMustBeZeroAddress),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for DiamondCutFacetErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) = <CannotAddFunctionToDiamondThatAlreadyExists as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::CannotAddFunctionToDiamondThatAlreadyExists(decoded));
            }
            if let Ok(decoded) =
                <CannotAddSelectorsToZeroAddress as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::CannotAddSelectorsToZeroAddress(decoded));
            }
            if let Ok(decoded) =
                <CannotRemoveFunctionThatDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                    data,
                )
            {
                return Ok(Self::CannotRemoveFunctionThatDoesNotExist(decoded));
            }
            if let Ok(decoded) =
                <CannotRemoveImmutableFunction as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::CannotRemoveImmutableFunction(decoded));
            }
            if let Ok(decoded) =
                <CannotReplaceFunctionThatDoesNotExists as ::ethers::core::abi::AbiDecode>::decode(
                    data,
                )
            {
                return Ok(Self::CannotReplaceFunctionThatDoesNotExists(decoded));
            }
            if let Ok(decoded) = <CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(
                    Self::CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet(
                        decoded,
                    ),
                );
            }
            if let Ok(decoded) = <CannotReplaceFunctionsFromFacetWithZeroAddress as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::CannotReplaceFunctionsFromFacetWithZeroAddress(decoded));
            }
            if let Ok(decoded) =
                <CannotReplaceImmutableFunction as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::CannotReplaceImmutableFunction(decoded));
            }
            if let Ok(decoded) =
                <IncorrectFacetCutAction as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::IncorrectFacetCutAction(decoded));
            }
            if let Ok(decoded) =
                <InitializationFunctionReverted as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::InitializationFunctionReverted(decoded));
            }
            if let Ok(decoded) =
                <NoBytecodeAtAddress as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::NoBytecodeAtAddress(decoded));
            }
            if let Ok(decoded) =
                <NoSelectorsProvidedForFacetForCut as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::NoSelectorsProvidedForFacetForCut(decoded));
            }
            if let Ok(decoded) = <NotContractOwner as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::NotContractOwner(decoded));
            }
            if let Ok(decoded) =
                <RemoveFacetAddressMustBeZeroAddress as ::ethers::core::abi::AbiDecode>::decode(
                    data,
                )
            {
                return Ok(Self::RemoveFacetAddressMustBeZeroAddress(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for DiamondCutFacetErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::CannotAddFunctionToDiamondThatAlreadyExists(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::CannotAddSelectorsToZeroAddress(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::CannotRemoveFunctionThatDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::CannotRemoveImmutableFunction(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::CannotReplaceFunctionThatDoesNotExists(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::CannotReplaceFunctionsFromFacetWithZeroAddress(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::CannotReplaceImmutableFunction(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::IncorrectFacetCutAction(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::InitializationFunctionReverted(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NoBytecodeAtAddress(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NoSelectorsProvidedForFacetForCut(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NotContractOwner(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RemoveFacetAddressMustBeZeroAddress(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for DiamondCutFacetErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector
                    == <CannotAddFunctionToDiamondThatAlreadyExists as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <CannotAddSelectorsToZeroAddress as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <CannotRemoveFunctionThatDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <CannotRemoveImmutableFunction as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <CannotReplaceFunctionThatDoesNotExists as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <CannotReplaceFunctionsFromFacetWithZeroAddress as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <CannotReplaceImmutableFunction as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <IncorrectFacetCutAction as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <InitializationFunctionReverted as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <NoBytecodeAtAddress as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <NoSelectorsProvidedForFacetForCut as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <NotContractOwner as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <RemoveFacetAddressMustBeZeroAddress as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for DiamondCutFacetErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::CannotAddFunctionToDiamondThatAlreadyExists(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::CannotAddSelectorsToZeroAddress(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::CannotRemoveFunctionThatDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::CannotRemoveImmutableFunction(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::CannotReplaceFunctionThatDoesNotExists(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::CannotReplaceFunctionsFromFacetWithZeroAddress(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::CannotReplaceImmutableFunction(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::IncorrectFacetCutAction(element) => ::core::fmt::Display::fmt(element, f),
                Self::InitializationFunctionReverted(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::NoBytecodeAtAddress(element) => ::core::fmt::Display::fmt(element, f),
                Self::NoSelectorsProvidedForFacetForCut(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::NotContractOwner(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemoveFacetAddressMustBeZeroAddress(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for DiamondCutFacetErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<CannotAddFunctionToDiamondThatAlreadyExists> for DiamondCutFacetErrors {
        fn from(value: CannotAddFunctionToDiamondThatAlreadyExists) -> Self {
            Self::CannotAddFunctionToDiamondThatAlreadyExists(value)
        }
    }
    impl ::core::convert::From<CannotAddSelectorsToZeroAddress> for DiamondCutFacetErrors {
        fn from(value: CannotAddSelectorsToZeroAddress) -> Self {
            Self::CannotAddSelectorsToZeroAddress(value)
        }
    }
    impl ::core::convert::From<CannotRemoveFunctionThatDoesNotExist> for DiamondCutFacetErrors {
        fn from(value: CannotRemoveFunctionThatDoesNotExist) -> Self {
            Self::CannotRemoveFunctionThatDoesNotExist(value)
        }
    }
    impl ::core::convert::From<CannotRemoveImmutableFunction> for DiamondCutFacetErrors {
        fn from(value: CannotRemoveImmutableFunction) -> Self {
            Self::CannotRemoveImmutableFunction(value)
        }
    }
    impl ::core::convert::From<CannotReplaceFunctionThatDoesNotExists> for DiamondCutFacetErrors {
        fn from(value: CannotReplaceFunctionThatDoesNotExists) -> Self {
            Self::CannotReplaceFunctionThatDoesNotExists(value)
        }
    }
    impl ::core::convert::From<CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet>
        for DiamondCutFacetErrors
    {
        fn from(value: CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet) -> Self {
            Self::CannotReplaceFunctionWithTheSameFunctionFromTheSameFacet(value)
        }
    }
    impl ::core::convert::From<CannotReplaceFunctionsFromFacetWithZeroAddress>
        for DiamondCutFacetErrors
    {
        fn from(value: CannotReplaceFunctionsFromFacetWithZeroAddress) -> Self {
            Self::CannotReplaceFunctionsFromFacetWithZeroAddress(value)
        }
    }
    impl ::core::convert::From<CannotReplaceImmutableFunction> for DiamondCutFacetErrors {
        fn from(value: CannotReplaceImmutableFunction) -> Self {
            Self::CannotReplaceImmutableFunction(value)
        }
    }
    impl ::core::convert::From<IncorrectFacetCutAction> for DiamondCutFacetErrors {
        fn from(value: IncorrectFacetCutAction) -> Self {
            Self::IncorrectFacetCutAction(value)
        }
    }
    impl ::core::convert::From<InitializationFunctionReverted> for DiamondCutFacetErrors {
        fn from(value: InitializationFunctionReverted) -> Self {
            Self::InitializationFunctionReverted(value)
        }
    }
    impl ::core::convert::From<NoBytecodeAtAddress> for DiamondCutFacetErrors {
        fn from(value: NoBytecodeAtAddress) -> Self {
            Self::NoBytecodeAtAddress(value)
        }
    }
    impl ::core::convert::From<NoSelectorsProvidedForFacetForCut> for DiamondCutFacetErrors {
        fn from(value: NoSelectorsProvidedForFacetForCut) -> Self {
            Self::NoSelectorsProvidedForFacetForCut(value)
        }
    }
    impl ::core::convert::From<NotContractOwner> for DiamondCutFacetErrors {
        fn from(value: NotContractOwner) -> Self {
            Self::NotContractOwner(value)
        }
    }
    impl ::core::convert::From<RemoveFacetAddressMustBeZeroAddress> for DiamondCutFacetErrors {
        fn from(value: RemoveFacetAddressMustBeZeroAddress) -> Self {
            Self::RemoveFacetAddressMustBeZeroAddress(value)
        }
    }
    #[derive(
        Clone,
        ::ethers::contract::EthEvent,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethevent(
        name = "DiamondCut",
        abi = "DiamondCut((address,uint8,bytes4[])[],address,bytes)"
    )]
    pub struct DiamondCut2Filter {
        pub diamond_cut: ::std::vec::Vec<FacetCut>,
        pub init: ::ethers::core::types::Address,
        pub calldata: ::ethers::core::types::Bytes,
    }
    ///Container type for all of the contract's events
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
    pub enum DiamondCutFacetEvents {
        DiamondCut2Filter(DiamondCut2Filter),
    }
    impl ::ethers::contract::EthLogDecode for DiamondCutFacetEvents {
        fn decode_log(
            log: &::ethers::core::abi::RawLog,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::Error> {
            if let Ok(decoded) = DiamondCut2Filter::decode_log(log) {
                return Ok(DiamondCutFacetEvents::DiamondCut2Filter(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData)
        }
    }
    impl ::core::fmt::Display for DiamondCutFacetEvents {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::DiamondCut2Filter(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<DiamondCut2Filter> for DiamondCutFacetEvents {
        fn from(value: DiamondCut2Filter) -> Self {
            Self::DiamondCut2Filter(value)
        }
    }
    ///Container type for all input parameters for the `diamondCut` function with signature `diamondCut((address,uint8,bytes4[])[],address,bytes)` and selector `0x1f931c1c`
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
        name = "diamondCut",
        abi = "diamondCut((address,uint8,bytes4[])[],address,bytes)"
    )]
    pub struct DiamondCutCall {
        pub diamond_cut: ::std::vec::Vec<FacetCut>,
        pub init: ::ethers::core::types::Address,
        pub calldata: ::ethers::core::types::Bytes,
    }
    ///`FacetCut(address,uint8,bytes4[])`
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
    pub struct FacetCut {
        pub facet_address: ::ethers::core::types::Address,
        pub action: u8,
        pub function_selectors: ::std::vec::Vec<[u8; 4]>,
    }
}
