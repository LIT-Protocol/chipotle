pub use account_config::*;
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
pub mod account_config {
    const _: () = {
        ::core::include_bytes!("./AccountConfig.json",);
    };
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("accountCount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("accountCount"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("accountExistsAndIsMutable"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("accountExistsAndIsMutable",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bool"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addActionToGroup"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("addActionToGroup"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("action"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("name"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("description"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addGroup"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("addGroup"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("name"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("description"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("cidHashes"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256[]"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pkpIds"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Address,
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address[]"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addPkpToGroup"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("addPkpToGroup"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pkpId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("adminApiPayerAccount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("adminApiPayerAccount",),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
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
                    ::std::borrow::ToOwned::to_owned("allPkpIdsAt"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("allPkpIdsAt"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("index"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
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
                    ::std::borrow::ToOwned::to_owned("apiPayerCount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("apiPayerCount"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("api_payers"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("api_payers"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
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
                    ::std::borrow::ToOwned::to_owned("creditApiKey"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("creditApiKey"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("amount"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("debitApiKey"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("debitApiKey"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("amount"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getPricing"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("getPricing"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("pricingItemId"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getWalletDerivation"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("getWalletDerivation",),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("walletAddress"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("indexToAccountHashAt"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("indexToAccountHashAt",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("index"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listActions"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("listActions"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                    ],),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct AppStorage.Metadata[]",),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listApiKeys"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("listApiKeys"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                        ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::String,
                                            ::ethers::core::abi::ethabi::ParamType::String,
                                        ],),
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ::ethers::core::abi::ethabi::ParamType::Array(
                                            ::std::boxed::Box::new(
                                                ::ethers::core::abi::ethabi::ParamType::Uint(
                                                    256usize
                                                ),
                                            ),
                                        ),
                                        ::ethers::core::abi::ethabi::ParamType::Bool,
                                        ::ethers::core::abi::ethabi::ParamType::Bool,
                                        ::ethers::core::abi::ethabi::ParamType::Bool,
                                        ::ethers::core::abi::ethabi::ParamType::Array(
                                            ::std::boxed::Box::new(
                                                ::ethers::core::abi::ethabi::ParamType::Uint(
                                                    256usize
                                                ),
                                            ),
                                        ),
                                        ::ethers::core::abi::ethabi::ParamType::Array(
                                            ::std::boxed::Box::new(
                                                ::ethers::core::abi::ethabi::ParamType::Uint(
                                                    256usize
                                                ),
                                            ),
                                        ),
                                        ::ethers::core::abi::ethabi::ParamType::Array(
                                            ::std::boxed::Box::new(
                                                ::ethers::core::abi::ethabi::ParamType::Uint(
                                                    256usize
                                                ),
                                            ),
                                        ),
                                    ],),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned(
                                    "struct ViewsFacet.UsageApiKeyReturn[]",
                                ),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listGroupContents"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("listGroupContents"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ::ethers::core::abi::ethabi::ParamType::String,
                                    ::ethers::core::abi::ethabi::ParamType::String,
                                ],),
                                ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ),
                                ),
                                ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Address,
                                    ),
                                ),
                            ],),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct ViewsFacet.GroupReturn",),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listGroups"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("listGroups"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                    ],),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct AppStorage.Metadata[]",),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listPkps"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("listPkps"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ::ethers::core::abi::ethabi::ParamType::Address,
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                    ],),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct AppStorage.PkpData[]",),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listWalletsInGroup"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("listWalletsInGroup"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ::ethers::core::abi::ethabi::ParamType::Address,
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                        ::ethers::core::abi::ethabi::ParamType::String,
                                    ],),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct AppStorage.PkpData[]",),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("newAccount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("newAccount"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("managed"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bool"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountName"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountDescription",),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("creatorWalletAddress",),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("pkpCount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("pkpCount"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("pricingAt"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("pricingAt"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("index"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("pricingOperator"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("pricingOperator"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
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
                    ::std::borrow::ToOwned::to_owned("rebalanceAmount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("rebalanceAmount"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("registerWalletDerivation"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("registerWalletDerivation",),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pkpId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("derivationPath"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("name"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("description"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("removeActionFromGroup"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("removeActionFromGroup",),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("action"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("removePkpFromGroup"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("removePkpFromGroup"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pkpId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("removeUsageApiKey"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("removeUsageApiKey"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("usageApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("requestedApiPayerCount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("requestedApiPayerCount",),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setAdminApiPayerAccount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setAdminApiPayerAccount",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("newAdminApiPayerAccount",),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setApiPayers"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setApiPayers"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("newApiPayers"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address[]"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setPricing"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setPricing"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pricingItemId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("price"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setPricingOperator"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setPricingOperator"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("newPricingOperator",),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setRebalanceAmount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setRebalanceAmount"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("newRebalanceAmount",),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setRequestedApiPayerCount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setRequestedApiPayerCount",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("newRequestedApiPayerCount",),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setUsageApiKey"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setUsageApiKey"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("usageApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("expiration"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("balance"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("name"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("description"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("createGroups"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bool"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("deleteGroups"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bool"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("createPKPs"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bool"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("manageIPFSIdsInGroups",),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256[]"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("addPkpToGroups"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256[]"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("removePkpFromGroups",),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256[]"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("executeInGroups"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256[]"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("updateActionMetadata"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("updateActionMetadata",),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("actionHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("name"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("description"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("updateGroup"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("updateGroup"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("name"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("description"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("updateUsageApiKeyMetadata"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("updateUsageApiKeyMetadata",),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("usageApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("name"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("description"),
                                kind: ::ethers::core::abi::ethabi::ParamType::String,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("string"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("AccountAlreadyExists"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("AccountAlreadyExists",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("AccountDoesNotExist"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("AccountDoesNotExist",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("ActionDoesNotExist"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("ActionDoesNotExist"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("cidHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("GroupDoesNotExist"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("GroupDoesNotExist"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("InsufficientBalance"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("InsufficientBalance",),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("amount"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("NoAccountAccess"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("NoAccountAccess"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("sender"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                        ],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("NotMasterAccount"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("NotMasterAccount"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("OnlyApiPayerOrOwner"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("OnlyApiPayerOrOwner",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("caller"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("OnlyApiPayerOrPricingOperator"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("OnlyApiPayerOrPricingOperator",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("caller"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("PkpDoesNotExist"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("PkpDoesNotExist"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("groupId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("pkpId"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                        ],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("UsageApiKeyDoesNotExist"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("UsageApiKeyDoesNotExist",),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("usageApiKeyHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                    },],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static ACCOUNTCONFIG_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    pub struct AccountConfig<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for AccountConfig<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for AccountConfig<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for AccountConfig<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for AccountConfig<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(AccountConfig))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> AccountConfig<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                ACCOUNTCONFIG_ABI.clone(),
                client,
            ))
        }
        ///Calls the contract's `accountCount` (0xe4af29fc) function
        pub fn account_count(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([228, 175, 41, 252], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `accountExistsAndIsMutable` (0x719fac43) function
        pub fn account_exists_and_is_mutable(
            &self,
            api_key_hash: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, bool> {
            self.0
                .method_hash([113, 159, 172, 67], api_key_hash)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addActionToGroup` (0x2fa92e41) function
        pub fn add_action_to_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            action: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [47, 169, 46, 65],
                    (account_api_key_hash, group_id, action, name, description),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addGroup` (0x2b12dee6) function
        pub fn add_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
            cid_hashes: ::std::vec::Vec<::ethers::core::types::U256>,
            pkp_ids: ::std::vec::Vec<::ethers::core::types::Address>,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [43, 18, 222, 230],
                    (account_api_key_hash, name, description, cid_hashes, pkp_ids),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addPkpToGroup` (0x7817d782) function
        pub fn add_pkp_to_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            pkp_id: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [120, 23, 215, 130],
                    (account_api_key_hash, group_id, pkp_id),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `adminApiPayerAccount` (0x383603fe) function
        pub fn admin_api_payer_account(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([56, 54, 3, 254], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `allPkpIdsAt` (0x62feae44) function
        pub fn all_pkp_ids_at(
            &self,
            index: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([98, 254, 174, 68], index)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `apiPayerCount` (0xb8037ffe) function
        pub fn api_payer_count(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([184, 3, 127, 254], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `api_payers` (0x93c8bc43) function
        pub fn api_payers(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<
            M,
            ::std::vec::Vec<::ethers::core::types::Address>,
        > {
            self.0
                .method_hash([147, 200, 188, 67], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `creditApiKey` (0x683f2de8) function
        pub fn credit_api_key(
            &self,
            api_key_hash: ::ethers::core::types::U256,
            amount: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([104, 63, 45, 232], (api_key_hash, amount))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `debitApiKey` (0x40b4d453) function
        pub fn debit_api_key(
            &self,
            api_key_hash: ::ethers::core::types::U256,
            amount: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([64, 180, 212, 83], (api_key_hash, amount))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `getPricing` (0xc12f1a42) function
        pub fn get_pricing(
            &self,
            pricing_item_id: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([193, 47, 26, 66], pricing_item_id)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `getWalletDerivation` (0x90222cad) function
        pub fn get_wallet_derivation(
            &self,
            api_key_hash: ::ethers::core::types::U256,
            wallet_address: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([144, 34, 44, 173], (api_key_hash, wallet_address))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `indexToAccountHashAt` (0x6fe1fb84) function
        pub fn index_to_account_hash_at(
            &self,
            index: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([111, 225, 251, 132], index)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `listActions` (0x542970ed) function
        pub fn list_actions(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            page_number: ::ethers::core::types::U256,
            page_size: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<Metadata>> {
            self.0
                .method_hash(
                    [84, 41, 112, 237],
                    (account_api_key_hash, group_id, page_number, page_size),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `listApiKeys` (0xc704668c) function
        pub fn list_api_keys(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            page_number: ::ethers::core::types::U256,
            page_size: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<UsageApiKeyReturn>>
        {
            self.0
                .method_hash(
                    [199, 4, 102, 140],
                    (account_api_key_hash, page_number, page_size),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `listGroupContents` (0x6fe9572e) function
        pub fn list_group_contents(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, GroupReturn> {
            self.0
                .method_hash([111, 233, 87, 46], (account_api_key_hash, group_id))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `listGroups` (0xf75c8b2d) function
        pub fn list_groups(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            page_number: ::ethers::core::types::U256,
            page_size: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<Metadata>> {
            self.0
                .method_hash(
                    [247, 92, 139, 45],
                    (account_api_key_hash, page_number, page_size),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `listPkps` (0x50409001) function
        pub fn list_pkps(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            page_number: ::ethers::core::types::U256,
            page_size: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<PkpData>> {
            self.0
                .method_hash(
                    [80, 64, 144, 1],
                    (account_api_key_hash, page_number, page_size),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `listWalletsInGroup` (0x291ff1ea) function
        pub fn list_wallets_in_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            page_number: ::ethers::core::types::U256,
            page_size: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<PkpData>> {
            self.0
                .method_hash(
                    [41, 31, 241, 234],
                    (account_api_key_hash, group_id, page_number, page_size),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `newAccount` (0x79312245) function
        pub fn new_account(
            &self,
            api_key_hash: ::ethers::core::types::U256,
            managed: bool,
            account_name: ::std::string::String,
            account_description: ::std::string::String,
            creator_wallet_address: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [121, 49, 34, 69],
                    (
                        api_key_hash,
                        managed,
                        account_name,
                        account_description,
                        creator_wallet_address,
                    ),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `pkpCount` (0x223a7962) function
        pub fn pkp_count(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([34, 58, 121, 98], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `pricingAt` (0xc1aff899) function
        pub fn pricing_at(
            &self,
            index: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([193, 175, 248, 153], index)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `pricingOperator` (0xcb53ad26) function
        pub fn pricing_operator(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([203, 83, 173, 38], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `rebalanceAmount` (0x3150289b) function
        pub fn rebalance_amount(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([49, 80, 40, 155], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `registerWalletDerivation` (0x92141552) function
        pub fn register_wallet_derivation(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            pkp_id: ::ethers::core::types::Address,
            derivation_path: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [146, 20, 21, 82],
                    (
                        account_api_key_hash,
                        pkp_id,
                        derivation_path,
                        name,
                        description,
                    ),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `removeActionFromGroup` (0x5d861c72) function
        pub fn remove_action_from_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            action: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([93, 134, 28, 114], (account_api_key_hash, group_id, action))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `removePkpFromGroup` (0x4a42c40a) function
        pub fn remove_pkp_from_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            pkp_id: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([74, 66, 196, 10], (account_api_key_hash, group_id, pkp_id))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `removeUsageApiKey` (0xc5f5b984) function
        pub fn remove_usage_api_key(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            usage_api_key_hash: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [197, 245, 185, 132],
                    (account_api_key_hash, usage_api_key_hash),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `requestedApiPayerCount` (0x34b7f87a) function
        pub fn requested_api_payer_count(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([52, 183, 248, 122], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setAdminApiPayerAccount` (0xc001bc79) function
        pub fn set_admin_api_payer_account(
            &self,
            new_admin_api_payer_account: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([192, 1, 188, 121], new_admin_api_payer_account)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setApiPayers` (0xae8c49a5) function
        pub fn set_api_payers(
            &self,
            new_api_payers: ::std::vec::Vec<::ethers::core::types::Address>,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([174, 140, 73, 165], new_api_payers)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setPricing` (0xca05588a) function
        pub fn set_pricing(
            &self,
            pricing_item_id: ::ethers::core::types::U256,
            price: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([202, 5, 88, 138], (pricing_item_id, price))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setPricingOperator` (0xe6ad2928) function
        pub fn set_pricing_operator(
            &self,
            new_pricing_operator: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([230, 173, 41, 40], new_pricing_operator)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setRebalanceAmount` (0xfdbfc9b7) function
        pub fn set_rebalance_amount(
            &self,
            new_rebalance_amount: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([253, 191, 201, 183], new_rebalance_amount)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setRequestedApiPayerCount` (0x52067859) function
        pub fn set_requested_api_payer_count(
            &self,
            new_requested_api_payer_count: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([82, 6, 120, 89], new_requested_api_payer_count)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setUsageApiKey` (0x2da0cf16) function
        pub fn set_usage_api_key(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            usage_api_key_hash: ::ethers::core::types::U256,
            expiration: ::ethers::core::types::U256,
            balance: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
            create_groups: bool,
            delete_groups: bool,
            create_pk_ps: bool,
            manage_ipfs_ids_in_groups: ::std::vec::Vec<::ethers::core::types::U256>,
            add_pkp_to_groups: ::std::vec::Vec<::ethers::core::types::U256>,
            remove_pkp_from_groups: ::std::vec::Vec<::ethers::core::types::U256>,
            execute_in_groups: ::std::vec::Vec<::ethers::core::types::U256>,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [45, 160, 207, 22],
                    (
                        account_api_key_hash,
                        usage_api_key_hash,
                        expiration,
                        balance,
                        name,
                        description,
                        create_groups,
                        delete_groups,
                        create_pk_ps,
                        manage_ipfs_ids_in_groups,
                        add_pkp_to_groups,
                        remove_pkp_from_groups,
                        execute_in_groups,
                    ),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `updateActionMetadata` (0xa6b6b672) function
        pub fn update_action_metadata(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            action_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [166, 182, 182, 114],
                    (
                        account_api_key_hash,
                        action_hash,
                        group_id,
                        name,
                        description,
                    ),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `updateGroup` (0x851b2a54) function
        pub fn update_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [133, 27, 42, 84],
                    (account_api_key_hash, group_id, name, description),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `updateUsageApiKeyMetadata` (0x6a3d77a9) function
        pub fn update_usage_api_key_metadata(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            usage_api_key_hash: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [106, 61, 119, 169],
                    (account_api_key_hash, usage_api_key_hash, name, description),
                )
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for AccountConfig<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `AccountAlreadyExists` with signature `AccountAlreadyExists(uint256)` and selector `0x8be1f3fb`
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
    #[etherror(name = "AccountAlreadyExists", abi = "AccountAlreadyExists(uint256)")]
    pub struct AccountAlreadyExists {
        pub api_key_hash: ::ethers::core::types::U256,
    }
    ///Custom Error type `AccountDoesNotExist` with signature `AccountDoesNotExist(uint256)` and selector `0xd4a84737`
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
    #[etherror(name = "AccountDoesNotExist", abi = "AccountDoesNotExist(uint256)")]
    pub struct AccountDoesNotExist {
        pub api_key_hash: ::ethers::core::types::U256,
    }
    ///Custom Error type `ActionDoesNotExist` with signature `ActionDoesNotExist(uint256,uint256,uint256)` and selector `0xef25d02d`
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
        name = "ActionDoesNotExist",
        abi = "ActionDoesNotExist(uint256,uint256,uint256)"
    )]
    pub struct ActionDoesNotExist {
        pub api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub cid_hash: ::ethers::core::types::U256,
    }
    ///Custom Error type `GroupDoesNotExist` with signature `GroupDoesNotExist(uint256,uint256)` and selector `0x931a85b3`
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
    #[etherror(name = "GroupDoesNotExist", abi = "GroupDoesNotExist(uint256,uint256)")]
    pub struct GroupDoesNotExist {
        pub api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
    }
    ///Custom Error type `InsufficientBalance` with signature `InsufficientBalance(uint256,uint256)` and selector `0xcf479181`
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
        name = "InsufficientBalance",
        abi = "InsufficientBalance(uint256,uint256)"
    )]
    pub struct InsufficientBalance {
        pub api_key_hash: ::ethers::core::types::U256,
        pub amount: ::ethers::core::types::U256,
    }
    ///Custom Error type `NoAccountAccess` with signature `NoAccountAccess(uint256,address)` and selector `0x7b0f9c07`
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
    #[etherror(name = "NoAccountAccess", abi = "NoAccountAccess(uint256,address)")]
    pub struct NoAccountAccess {
        pub api_key_hash: ::ethers::core::types::U256,
        pub sender: ::ethers::core::types::Address,
    }
    ///Custom Error type `NotMasterAccount` with signature `NotMasterAccount(uint256)` and selector `0x1d0932ee`
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
    #[etherror(name = "NotMasterAccount", abi = "NotMasterAccount(uint256)")]
    pub struct NotMasterAccount {
        pub api_key_hash: ::ethers::core::types::U256,
    }
    ///Custom Error type `OnlyApiPayerOrOwner` with signature `OnlyApiPayerOrOwner(address)` and selector `0x2d87faed`
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
    #[etherror(name = "OnlyApiPayerOrOwner", abi = "OnlyApiPayerOrOwner(address)")]
    pub struct OnlyApiPayerOrOwner {
        pub caller: ::ethers::core::types::Address,
    }
    ///Custom Error type `OnlyApiPayerOrPricingOperator` with signature `OnlyApiPayerOrPricingOperator(address)` and selector `0x9ed88e07`
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
        name = "OnlyApiPayerOrPricingOperator",
        abi = "OnlyApiPayerOrPricingOperator(address)"
    )]
    pub struct OnlyApiPayerOrPricingOperator {
        pub caller: ::ethers::core::types::Address,
    }
    ///Custom Error type `PkpDoesNotExist` with signature `PkpDoesNotExist(uint256,uint256,address)` and selector `0x2593917a`
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
        name = "PkpDoesNotExist",
        abi = "PkpDoesNotExist(uint256,uint256,address)"
    )]
    pub struct PkpDoesNotExist {
        pub api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub pkp_id: ::ethers::core::types::Address,
    }
    ///Custom Error type `UsageApiKeyDoesNotExist` with signature `UsageApiKeyDoesNotExist(uint256,uint256)` and selector `0x748e712a`
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
        name = "UsageApiKeyDoesNotExist",
        abi = "UsageApiKeyDoesNotExist(uint256,uint256)"
    )]
    pub struct UsageApiKeyDoesNotExist {
        pub api_key_hash: ::ethers::core::types::U256,
        pub usage_api_key_hash: ::ethers::core::types::U256,
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
    pub enum AccountConfigErrors {
        AccountAlreadyExists(AccountAlreadyExists),
        AccountDoesNotExist(AccountDoesNotExist),
        ActionDoesNotExist(ActionDoesNotExist),
        GroupDoesNotExist(GroupDoesNotExist),
        InsufficientBalance(InsufficientBalance),
        NoAccountAccess(NoAccountAccess),
        NotMasterAccount(NotMasterAccount),
        OnlyApiPayerOrOwner(OnlyApiPayerOrOwner),
        OnlyApiPayerOrPricingOperator(OnlyApiPayerOrPricingOperator),
        PkpDoesNotExist(PkpDoesNotExist),
        UsageApiKeyDoesNotExist(UsageApiKeyDoesNotExist),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for AccountConfigErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) =
                <AccountAlreadyExists as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AccountAlreadyExists(decoded));
            }
            if let Ok(decoded) =
                <AccountDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AccountDoesNotExist(decoded));
            }
            if let Ok(decoded) =
                <ActionDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ActionDoesNotExist(decoded));
            }
            if let Ok(decoded) = <GroupDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::GroupDoesNotExist(decoded));
            }
            if let Ok(decoded) =
                <InsufficientBalance as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::InsufficientBalance(decoded));
            }
            if let Ok(decoded) = <NoAccountAccess as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::NoAccountAccess(decoded));
            }
            if let Ok(decoded) = <NotMasterAccount as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::NotMasterAccount(decoded));
            }
            if let Ok(decoded) =
                <OnlyApiPayerOrOwner as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::OnlyApiPayerOrOwner(decoded));
            }
            if let Ok(decoded) =
                <OnlyApiPayerOrPricingOperator as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::OnlyApiPayerOrPricingOperator(decoded));
            }
            if let Ok(decoded) = <PkpDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::PkpDoesNotExist(decoded));
            }
            if let Ok(decoded) =
                <UsageApiKeyDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::UsageApiKeyDoesNotExist(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for AccountConfigErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::AccountAlreadyExists(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AccountDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ActionDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::GroupDoesNotExist(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::InsufficientBalance(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NoAccountAccess(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::NotMasterAccount(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::OnlyApiPayerOrOwner(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::OnlyApiPayerOrPricingOperator(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::PkpDoesNotExist(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::UsageApiKeyDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for AccountConfigErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector
                    == <AccountAlreadyExists as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <AccountDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <ActionDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <GroupDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <InsufficientBalance as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <NoAccountAccess as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <NotMasterAccount as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <OnlyApiPayerOrOwner as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <OnlyApiPayerOrPricingOperator as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <PkpDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <UsageApiKeyDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for AccountConfigErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AccountAlreadyExists(element) => ::core::fmt::Display::fmt(element, f),
                Self::AccountDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::ActionDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::GroupDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::InsufficientBalance(element) => ::core::fmt::Display::fmt(element, f),
                Self::NoAccountAccess(element) => ::core::fmt::Display::fmt(element, f),
                Self::NotMasterAccount(element) => ::core::fmt::Display::fmt(element, f),
                Self::OnlyApiPayerOrOwner(element) => ::core::fmt::Display::fmt(element, f),
                Self::OnlyApiPayerOrPricingOperator(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::PkpDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::UsageApiKeyDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for AccountConfigErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<AccountAlreadyExists> for AccountConfigErrors {
        fn from(value: AccountAlreadyExists) -> Self {
            Self::AccountAlreadyExists(value)
        }
    }
    impl ::core::convert::From<AccountDoesNotExist> for AccountConfigErrors {
        fn from(value: AccountDoesNotExist) -> Self {
            Self::AccountDoesNotExist(value)
        }
    }
    impl ::core::convert::From<ActionDoesNotExist> for AccountConfigErrors {
        fn from(value: ActionDoesNotExist) -> Self {
            Self::ActionDoesNotExist(value)
        }
    }
    impl ::core::convert::From<GroupDoesNotExist> for AccountConfigErrors {
        fn from(value: GroupDoesNotExist) -> Self {
            Self::GroupDoesNotExist(value)
        }
    }
    impl ::core::convert::From<InsufficientBalance> for AccountConfigErrors {
        fn from(value: InsufficientBalance) -> Self {
            Self::InsufficientBalance(value)
        }
    }
    impl ::core::convert::From<NoAccountAccess> for AccountConfigErrors {
        fn from(value: NoAccountAccess) -> Self {
            Self::NoAccountAccess(value)
        }
    }
    impl ::core::convert::From<NotMasterAccount> for AccountConfigErrors {
        fn from(value: NotMasterAccount) -> Self {
            Self::NotMasterAccount(value)
        }
    }
    impl ::core::convert::From<OnlyApiPayerOrOwner> for AccountConfigErrors {
        fn from(value: OnlyApiPayerOrOwner) -> Self {
            Self::OnlyApiPayerOrOwner(value)
        }
    }
    impl ::core::convert::From<OnlyApiPayerOrPricingOperator> for AccountConfigErrors {
        fn from(value: OnlyApiPayerOrPricingOperator) -> Self {
            Self::OnlyApiPayerOrPricingOperator(value)
        }
    }
    impl ::core::convert::From<PkpDoesNotExist> for AccountConfigErrors {
        fn from(value: PkpDoesNotExist) -> Self {
            Self::PkpDoesNotExist(value)
        }
    }
    impl ::core::convert::From<UsageApiKeyDoesNotExist> for AccountConfigErrors {
        fn from(value: UsageApiKeyDoesNotExist) -> Self {
            Self::UsageApiKeyDoesNotExist(value)
        }
    }
    ///Container type for all input parameters for the `accountCount` function with signature `accountCount()` and selector `0xe4af29fc`
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
    #[ethcall(name = "accountCount", abi = "accountCount()")]
    pub struct AccountCountCall;
    ///Container type for all input parameters for the `accountExistsAndIsMutable` function with signature `accountExistsAndIsMutable(uint256)` and selector `0x719fac43`
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
        name = "accountExistsAndIsMutable",
        abi = "accountExistsAndIsMutable(uint256)"
    )]
    pub struct AccountExistsAndIsMutableCall {
        pub api_key_hash: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `addActionToGroup` function with signature `addActionToGroup(uint256,uint256,uint256,string,string)` and selector `0x2fa92e41`
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
        name = "addActionToGroup",
        abi = "addActionToGroup(uint256,uint256,uint256,string,string)"
    )]
    pub struct AddActionToGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub action: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
    ///Container type for all input parameters for the `addGroup` function with signature `addGroup(uint256,string,string,uint256[],address[])` and selector `0x2b12dee6`
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
        name = "addGroup",
        abi = "addGroup(uint256,string,string,uint256[],address[])"
    )]
    pub struct AddGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
        pub cid_hashes: ::std::vec::Vec<::ethers::core::types::U256>,
        pub pkp_ids: ::std::vec::Vec<::ethers::core::types::Address>,
    }
    ///Container type for all input parameters for the `addPkpToGroup` function with signature `addPkpToGroup(uint256,uint256,address)` and selector `0x7817d782`
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
    #[ethcall(name = "addPkpToGroup", abi = "addPkpToGroup(uint256,uint256,address)")]
    pub struct AddPkpToGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub pkp_id: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `adminApiPayerAccount` function with signature `adminApiPayerAccount()` and selector `0x383603fe`
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
    #[ethcall(name = "adminApiPayerAccount", abi = "adminApiPayerAccount()")]
    pub struct AdminApiPayerAccountCall;
    ///Container type for all input parameters for the `allPkpIdsAt` function with signature `allPkpIdsAt(uint256)` and selector `0x62feae44`
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
    #[ethcall(name = "allPkpIdsAt", abi = "allPkpIdsAt(uint256)")]
    pub struct AllPkpIdsAtCall {
        pub index: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `apiPayerCount` function with signature `apiPayerCount()` and selector `0xb8037ffe`
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
    #[ethcall(name = "apiPayerCount", abi = "apiPayerCount()")]
    pub struct ApiPayerCountCall;
    ///Container type for all input parameters for the `api_payers` function with signature `api_payers()` and selector `0x93c8bc43`
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
    #[ethcall(name = "api_payers", abi = "api_payers()")]
    pub struct ApiPayersCall;
    ///Container type for all input parameters for the `creditApiKey` function with signature `creditApiKey(uint256,uint256)` and selector `0x683f2de8`
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
    #[ethcall(name = "creditApiKey", abi = "creditApiKey(uint256,uint256)")]
    pub struct CreditApiKeyCall {
        pub api_key_hash: ::ethers::core::types::U256,
        pub amount: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `debitApiKey` function with signature `debitApiKey(uint256,uint256)` and selector `0x40b4d453`
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
    #[ethcall(name = "debitApiKey", abi = "debitApiKey(uint256,uint256)")]
    pub struct DebitApiKeyCall {
        pub api_key_hash: ::ethers::core::types::U256,
        pub amount: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `getPricing` function with signature `getPricing(uint256)` and selector `0xc12f1a42`
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
    #[ethcall(name = "getPricing", abi = "getPricing(uint256)")]
    pub struct GetPricingCall {
        pub pricing_item_id: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `getWalletDerivation` function with signature `getWalletDerivation(uint256,address)` and selector `0x90222cad`
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
        name = "getWalletDerivation",
        abi = "getWalletDerivation(uint256,address)"
    )]
    pub struct GetWalletDerivationCall {
        pub api_key_hash: ::ethers::core::types::U256,
        pub wallet_address: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `indexToAccountHashAt` function with signature `indexToAccountHashAt(uint256)` and selector `0x6fe1fb84`
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
    #[ethcall(name = "indexToAccountHashAt", abi = "indexToAccountHashAt(uint256)")]
    pub struct IndexToAccountHashAtCall {
        pub index: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `listActions` function with signature `listActions(uint256,uint256,uint256,uint256)` and selector `0x542970ed`
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
        name = "listActions",
        abi = "listActions(uint256,uint256,uint256,uint256)"
    )]
    pub struct ListActionsCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub page_number: ::ethers::core::types::U256,
        pub page_size: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `listApiKeys` function with signature `listApiKeys(uint256,uint256,uint256)` and selector `0xc704668c`
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
    #[ethcall(name = "listApiKeys", abi = "listApiKeys(uint256,uint256,uint256)")]
    pub struct ListApiKeysCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub page_number: ::ethers::core::types::U256,
        pub page_size: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `listGroupContents` function with signature `listGroupContents(uint256,uint256)` and selector `0x6fe9572e`
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
    #[ethcall(name = "listGroupContents", abi = "listGroupContents(uint256,uint256)")]
    pub struct ListGroupContentsCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `listGroups` function with signature `listGroups(uint256,uint256,uint256)` and selector `0xf75c8b2d`
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
    #[ethcall(name = "listGroups", abi = "listGroups(uint256,uint256,uint256)")]
    pub struct ListGroupsCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub page_number: ::ethers::core::types::U256,
        pub page_size: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `listPkps` function with signature `listPkps(uint256,uint256,uint256)` and selector `0x50409001`
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
    #[ethcall(name = "listPkps", abi = "listPkps(uint256,uint256,uint256)")]
    pub struct ListPkpsCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub page_number: ::ethers::core::types::U256,
        pub page_size: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `listWalletsInGroup` function with signature `listWalletsInGroup(uint256,uint256,uint256,uint256)` and selector `0x291ff1ea`
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
        name = "listWalletsInGroup",
        abi = "listWalletsInGroup(uint256,uint256,uint256,uint256)"
    )]
    pub struct ListWalletsInGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub page_number: ::ethers::core::types::U256,
        pub page_size: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `newAccount` function with signature `newAccount(uint256,bool,string,string,address)` and selector `0x79312245`
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
        name = "newAccount",
        abi = "newAccount(uint256,bool,string,string,address)"
    )]
    pub struct NewAccountCall {
        pub api_key_hash: ::ethers::core::types::U256,
        pub managed: bool,
        pub account_name: ::std::string::String,
        pub account_description: ::std::string::String,
        pub creator_wallet_address: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `pkpCount` function with signature `pkpCount()` and selector `0x223a7962`
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
    #[ethcall(name = "pkpCount", abi = "pkpCount()")]
    pub struct PkpCountCall;
    ///Container type for all input parameters for the `pricingAt` function with signature `pricingAt(uint256)` and selector `0xc1aff899`
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
    #[ethcall(name = "pricingAt", abi = "pricingAt(uint256)")]
    pub struct PricingAtCall {
        pub index: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `pricingOperator` function with signature `pricingOperator()` and selector `0xcb53ad26`
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
    #[ethcall(name = "pricingOperator", abi = "pricingOperator()")]
    pub struct PricingOperatorCall;
    ///Container type for all input parameters for the `rebalanceAmount` function with signature `rebalanceAmount()` and selector `0x3150289b`
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
    #[ethcall(name = "rebalanceAmount", abi = "rebalanceAmount()")]
    pub struct RebalanceAmountCall;
    ///Container type for all input parameters for the `registerWalletDerivation` function with signature `registerWalletDerivation(uint256,address,uint256,string,string)` and selector `0x92141552`
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
        name = "registerWalletDerivation",
        abi = "registerWalletDerivation(uint256,address,uint256,string,string)"
    )]
    pub struct RegisterWalletDerivationCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub pkp_id: ::ethers::core::types::Address,
        pub derivation_path: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
    ///Container type for all input parameters for the `removeActionFromGroup` function with signature `removeActionFromGroup(uint256,uint256,uint256)` and selector `0x5d861c72`
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
        name = "removeActionFromGroup",
        abi = "removeActionFromGroup(uint256,uint256,uint256)"
    )]
    pub struct RemoveActionFromGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub action: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `removePkpFromGroup` function with signature `removePkpFromGroup(uint256,uint256,address)` and selector `0x4a42c40a`
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
        name = "removePkpFromGroup",
        abi = "removePkpFromGroup(uint256,uint256,address)"
    )]
    pub struct RemovePkpFromGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub pkp_id: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `removeUsageApiKey` function with signature `removeUsageApiKey(uint256,uint256)` and selector `0xc5f5b984`
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
    #[ethcall(name = "removeUsageApiKey", abi = "removeUsageApiKey(uint256,uint256)")]
    pub struct RemoveUsageApiKeyCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub usage_api_key_hash: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `requestedApiPayerCount` function with signature `requestedApiPayerCount()` and selector `0x34b7f87a`
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
    #[ethcall(name = "requestedApiPayerCount", abi = "requestedApiPayerCount()")]
    pub struct RequestedApiPayerCountCall;
    ///Container type for all input parameters for the `setAdminApiPayerAccount` function with signature `setAdminApiPayerAccount(address)` and selector `0xc001bc79`
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
        name = "setAdminApiPayerAccount",
        abi = "setAdminApiPayerAccount(address)"
    )]
    pub struct SetAdminApiPayerAccountCall {
        pub new_admin_api_payer_account: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `setApiPayers` function with signature `setApiPayers(address[])` and selector `0xae8c49a5`
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
    #[ethcall(name = "setApiPayers", abi = "setApiPayers(address[])")]
    pub struct SetApiPayersCall {
        pub new_api_payers: ::std::vec::Vec<::ethers::core::types::Address>,
    }
    ///Container type for all input parameters for the `setPricing` function with signature `setPricing(uint256,uint256)` and selector `0xca05588a`
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
    #[ethcall(name = "setPricing", abi = "setPricing(uint256,uint256)")]
    pub struct SetPricingCall {
        pub pricing_item_id: ::ethers::core::types::U256,
        pub price: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `setPricingOperator` function with signature `setPricingOperator(address)` and selector `0xe6ad2928`
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
    #[ethcall(name = "setPricingOperator", abi = "setPricingOperator(address)")]
    pub struct SetPricingOperatorCall {
        pub new_pricing_operator: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `setRebalanceAmount` function with signature `setRebalanceAmount(uint256)` and selector `0xfdbfc9b7`
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
    #[ethcall(name = "setRebalanceAmount", abi = "setRebalanceAmount(uint256)")]
    pub struct SetRebalanceAmountCall {
        pub new_rebalance_amount: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `setRequestedApiPayerCount` function with signature `setRequestedApiPayerCount(uint256)` and selector `0x52067859`
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
        name = "setRequestedApiPayerCount",
        abi = "setRequestedApiPayerCount(uint256)"
    )]
    pub struct SetRequestedApiPayerCountCall {
        pub new_requested_api_payer_count: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `setUsageApiKey` function with signature `setUsageApiKey(uint256,uint256,uint256,uint256,string,string,bool,bool,bool,uint256[],uint256[],uint256[],uint256[])` and selector `0x2da0cf16`
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
        name = "setUsageApiKey",
        abi = "setUsageApiKey(uint256,uint256,uint256,uint256,string,string,bool,bool,bool,uint256[],uint256[],uint256[],uint256[])"
    )]
    pub struct SetUsageApiKeyCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub usage_api_key_hash: ::ethers::core::types::U256,
        pub expiration: ::ethers::core::types::U256,
        pub balance: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
        pub create_groups: bool,
        pub delete_groups: bool,
        pub create_pk_ps: bool,
        pub manage_ipfs_ids_in_groups: ::std::vec::Vec<::ethers::core::types::U256>,
        pub add_pkp_to_groups: ::std::vec::Vec<::ethers::core::types::U256>,
        pub remove_pkp_from_groups: ::std::vec::Vec<::ethers::core::types::U256>,
        pub execute_in_groups: ::std::vec::Vec<::ethers::core::types::U256>,
    }
    ///Container type for all input parameters for the `updateActionMetadata` function with signature `updateActionMetadata(uint256,uint256,uint256,string,string)` and selector `0xa6b6b672`
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
        name = "updateActionMetadata",
        abi = "updateActionMetadata(uint256,uint256,uint256,string,string)"
    )]
    pub struct UpdateActionMetadataCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub action_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
    ///Container type for all input parameters for the `updateGroup` function with signature `updateGroup(uint256,uint256,string,string)` and selector `0x851b2a54`
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
        name = "updateGroup",
        abi = "updateGroup(uint256,uint256,string,string)"
    )]
    pub struct UpdateGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
    ///Container type for all input parameters for the `updateUsageApiKeyMetadata` function with signature `updateUsageApiKeyMetadata(uint256,uint256,string,string)` and selector `0x6a3d77a9`
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
        name = "updateUsageApiKeyMetadata",
        abi = "updateUsageApiKeyMetadata(uint256,uint256,string,string)"
    )]
    pub struct UpdateUsageApiKeyMetadataCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub usage_api_key_hash: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
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
    pub enum AccountConfigCalls {
        AccountCount(AccountCountCall),
        AccountExistsAndIsMutable(AccountExistsAndIsMutableCall),
        AddActionToGroup(AddActionToGroupCall),
        AddGroup(AddGroupCall),
        AddPkpToGroup(AddPkpToGroupCall),
        AdminApiPayerAccount(AdminApiPayerAccountCall),
        AllPkpIdsAt(AllPkpIdsAtCall),
        ApiPayerCount(ApiPayerCountCall),
        ApiPayers(ApiPayersCall),
        CreditApiKey(CreditApiKeyCall),
        DebitApiKey(DebitApiKeyCall),
        GetPricing(GetPricingCall),
        GetWalletDerivation(GetWalletDerivationCall),
        IndexToAccountHashAt(IndexToAccountHashAtCall),
        ListActions(ListActionsCall),
        ListApiKeys(ListApiKeysCall),
        ListGroupContents(ListGroupContentsCall),
        ListGroups(ListGroupsCall),
        ListPkps(ListPkpsCall),
        ListWalletsInGroup(ListWalletsInGroupCall),
        NewAccount(NewAccountCall),
        PkpCount(PkpCountCall),
        PricingAt(PricingAtCall),
        PricingOperator(PricingOperatorCall),
        RebalanceAmount(RebalanceAmountCall),
        RegisterWalletDerivation(RegisterWalletDerivationCall),
        RemoveActionFromGroup(RemoveActionFromGroupCall),
        RemovePkpFromGroup(RemovePkpFromGroupCall),
        RemoveUsageApiKey(RemoveUsageApiKeyCall),
        RequestedApiPayerCount(RequestedApiPayerCountCall),
        SetAdminApiPayerAccount(SetAdminApiPayerAccountCall),
        SetApiPayers(SetApiPayersCall),
        SetPricing(SetPricingCall),
        SetPricingOperator(SetPricingOperatorCall),
        SetRebalanceAmount(SetRebalanceAmountCall),
        SetRequestedApiPayerCount(SetRequestedApiPayerCountCall),
        SetUsageApiKey(SetUsageApiKeyCall),
        UpdateActionMetadata(UpdateActionMetadataCall),
        UpdateGroup(UpdateGroupCall),
        UpdateUsageApiKeyMetadata(UpdateUsageApiKeyMetadataCall),
    }
    impl ::ethers::core::abi::AbiDecode for AccountConfigCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <AccountCountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AccountCount(decoded));
            }
            if let Ok(decoded) =
                <AccountExistsAndIsMutableCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AccountExistsAndIsMutable(decoded));
            }
            if let Ok(decoded) =
                <AddActionToGroupCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AddActionToGroup(decoded));
            }
            if let Ok(decoded) = <AddGroupCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::AddGroup(decoded));
            }
            if let Ok(decoded) = <AddPkpToGroupCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AddPkpToGroup(decoded));
            }
            if let Ok(decoded) =
                <AdminApiPayerAccountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AdminApiPayerAccount(decoded));
            }
            if let Ok(decoded) = <AllPkpIdsAtCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::AllPkpIdsAt(decoded));
            }
            if let Ok(decoded) = <ApiPayerCountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ApiPayerCount(decoded));
            }
            if let Ok(decoded) = <ApiPayersCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::ApiPayers(decoded));
            }
            if let Ok(decoded) = <CreditApiKeyCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::CreditApiKey(decoded));
            }
            if let Ok(decoded) = <DebitApiKeyCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::DebitApiKey(decoded));
            }
            if let Ok(decoded) = <GetPricingCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::GetPricing(decoded));
            }
            if let Ok(decoded) =
                <GetWalletDerivationCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::GetWalletDerivation(decoded));
            }
            if let Ok(decoded) =
                <IndexToAccountHashAtCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::IndexToAccountHashAt(decoded));
            }
            if let Ok(decoded) = <ListActionsCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::ListActions(decoded));
            }
            if let Ok(decoded) = <ListApiKeysCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::ListApiKeys(decoded));
            }
            if let Ok(decoded) =
                <ListGroupContentsCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ListGroupContents(decoded));
            }
            if let Ok(decoded) = <ListGroupsCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::ListGroups(decoded));
            }
            if let Ok(decoded) = <ListPkpsCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::ListPkps(decoded));
            }
            if let Ok(decoded) =
                <ListWalletsInGroupCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ListWalletsInGroup(decoded));
            }
            if let Ok(decoded) = <NewAccountCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::NewAccount(decoded));
            }
            if let Ok(decoded) = <PkpCountCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::PkpCount(decoded));
            }
            if let Ok(decoded) = <PricingAtCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::PricingAt(decoded));
            }
            if let Ok(decoded) =
                <PricingOperatorCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::PricingOperator(decoded));
            }
            if let Ok(decoded) =
                <RebalanceAmountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RebalanceAmount(decoded));
            }
            if let Ok(decoded) =
                <RegisterWalletDerivationCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RegisterWalletDerivation(decoded));
            }
            if let Ok(decoded) =
                <RemoveActionFromGroupCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RemoveActionFromGroup(decoded));
            }
            if let Ok(decoded) =
                <RemovePkpFromGroupCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RemovePkpFromGroup(decoded));
            }
            if let Ok(decoded) =
                <RemoveUsageApiKeyCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RemoveUsageApiKey(decoded));
            }
            if let Ok(decoded) =
                <RequestedApiPayerCountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RequestedApiPayerCount(decoded));
            }
            if let Ok(decoded) =
                <SetAdminApiPayerAccountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SetAdminApiPayerAccount(decoded));
            }
            if let Ok(decoded) = <SetApiPayersCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SetApiPayers(decoded));
            }
            if let Ok(decoded) = <SetPricingCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::SetPricing(decoded));
            }
            if let Ok(decoded) =
                <SetPricingOperatorCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SetPricingOperator(decoded));
            }
            if let Ok(decoded) =
                <SetRebalanceAmountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SetRebalanceAmount(decoded));
            }
            if let Ok(decoded) =
                <SetRequestedApiPayerCountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SetRequestedApiPayerCount(decoded));
            }
            if let Ok(decoded) =
                <SetUsageApiKeyCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SetUsageApiKey(decoded));
            }
            if let Ok(decoded) =
                <UpdateActionMetadataCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::UpdateActionMetadata(decoded));
            }
            if let Ok(decoded) = <UpdateGroupCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::UpdateGroup(decoded));
            }
            if let Ok(decoded) =
                <UpdateUsageApiKeyMetadataCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::UpdateUsageApiKeyMetadata(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for AccountConfigCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::AccountCount(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::AccountExistsAndIsMutable(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddActionToGroup(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::AddGroup(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::AddPkpToGroup(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::AdminApiPayerAccount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AllPkpIdsAt(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ApiPayerCount(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ApiPayers(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::CreditApiKey(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::DebitApiKey(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetPricing(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetWalletDerivation(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::IndexToAccountHashAt(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ListActions(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ListApiKeys(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ListGroupContents(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ListGroups(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ListPkps(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ListWalletsInGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NewAccount(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::PkpCount(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::PricingAt(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::PricingOperator(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RebalanceAmount(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RegisterWalletDerivation(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemoveActionFromGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemovePkpFromGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemoveUsageApiKey(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RequestedApiPayerCount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::SetAdminApiPayerAccount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::SetApiPayers(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::SetPricing(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::SetPricingOperator(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::SetRebalanceAmount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::SetRequestedApiPayerCount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::SetUsageApiKey(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::UpdateActionMetadata(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UpdateGroup(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::UpdateUsageApiKeyMetadata(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
            }
        }
    }
    impl ::core::fmt::Display for AccountConfigCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AccountCount(element) => ::core::fmt::Display::fmt(element, f),
                Self::AccountExistsAndIsMutable(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddActionToGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddPkpToGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::AdminApiPayerAccount(element) => ::core::fmt::Display::fmt(element, f),
                Self::AllPkpIdsAt(element) => ::core::fmt::Display::fmt(element, f),
                Self::ApiPayerCount(element) => ::core::fmt::Display::fmt(element, f),
                Self::ApiPayers(element) => ::core::fmt::Display::fmt(element, f),
                Self::CreditApiKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::DebitApiKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetPricing(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetWalletDerivation(element) => ::core::fmt::Display::fmt(element, f),
                Self::IndexToAccountHashAt(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListActions(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListApiKeys(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListGroupContents(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListGroups(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListPkps(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListWalletsInGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::NewAccount(element) => ::core::fmt::Display::fmt(element, f),
                Self::PkpCount(element) => ::core::fmt::Display::fmt(element, f),
                Self::PricingAt(element) => ::core::fmt::Display::fmt(element, f),
                Self::PricingOperator(element) => ::core::fmt::Display::fmt(element, f),
                Self::RebalanceAmount(element) => ::core::fmt::Display::fmt(element, f),
                Self::RegisterWalletDerivation(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemoveActionFromGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemovePkpFromGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemoveUsageApiKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::RequestedApiPayerCount(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetAdminApiPayerAccount(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetApiPayers(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetPricing(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetPricingOperator(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetRebalanceAmount(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetRequestedApiPayerCount(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetUsageApiKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpdateActionMetadata(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpdateGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpdateUsageApiKeyMetadata(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<AccountCountCall> for AccountConfigCalls {
        fn from(value: AccountCountCall) -> Self {
            Self::AccountCount(value)
        }
    }
    impl ::core::convert::From<AccountExistsAndIsMutableCall> for AccountConfigCalls {
        fn from(value: AccountExistsAndIsMutableCall) -> Self {
            Self::AccountExistsAndIsMutable(value)
        }
    }
    impl ::core::convert::From<AddActionToGroupCall> for AccountConfigCalls {
        fn from(value: AddActionToGroupCall) -> Self {
            Self::AddActionToGroup(value)
        }
    }
    impl ::core::convert::From<AddGroupCall> for AccountConfigCalls {
        fn from(value: AddGroupCall) -> Self {
            Self::AddGroup(value)
        }
    }
    impl ::core::convert::From<AddPkpToGroupCall> for AccountConfigCalls {
        fn from(value: AddPkpToGroupCall) -> Self {
            Self::AddPkpToGroup(value)
        }
    }
    impl ::core::convert::From<AdminApiPayerAccountCall> for AccountConfigCalls {
        fn from(value: AdminApiPayerAccountCall) -> Self {
            Self::AdminApiPayerAccount(value)
        }
    }
    impl ::core::convert::From<AllPkpIdsAtCall> for AccountConfigCalls {
        fn from(value: AllPkpIdsAtCall) -> Self {
            Self::AllPkpIdsAt(value)
        }
    }
    impl ::core::convert::From<ApiPayerCountCall> for AccountConfigCalls {
        fn from(value: ApiPayerCountCall) -> Self {
            Self::ApiPayerCount(value)
        }
    }
    impl ::core::convert::From<ApiPayersCall> for AccountConfigCalls {
        fn from(value: ApiPayersCall) -> Self {
            Self::ApiPayers(value)
        }
    }
    impl ::core::convert::From<CreditApiKeyCall> for AccountConfigCalls {
        fn from(value: CreditApiKeyCall) -> Self {
            Self::CreditApiKey(value)
        }
    }
    impl ::core::convert::From<DebitApiKeyCall> for AccountConfigCalls {
        fn from(value: DebitApiKeyCall) -> Self {
            Self::DebitApiKey(value)
        }
    }
    impl ::core::convert::From<GetPricingCall> for AccountConfigCalls {
        fn from(value: GetPricingCall) -> Self {
            Self::GetPricing(value)
        }
    }
    impl ::core::convert::From<GetWalletDerivationCall> for AccountConfigCalls {
        fn from(value: GetWalletDerivationCall) -> Self {
            Self::GetWalletDerivation(value)
        }
    }
    impl ::core::convert::From<IndexToAccountHashAtCall> for AccountConfigCalls {
        fn from(value: IndexToAccountHashAtCall) -> Self {
            Self::IndexToAccountHashAt(value)
        }
    }
    impl ::core::convert::From<ListActionsCall> for AccountConfigCalls {
        fn from(value: ListActionsCall) -> Self {
            Self::ListActions(value)
        }
    }
    impl ::core::convert::From<ListApiKeysCall> for AccountConfigCalls {
        fn from(value: ListApiKeysCall) -> Self {
            Self::ListApiKeys(value)
        }
    }
    impl ::core::convert::From<ListGroupContentsCall> for AccountConfigCalls {
        fn from(value: ListGroupContentsCall) -> Self {
            Self::ListGroupContents(value)
        }
    }
    impl ::core::convert::From<ListGroupsCall> for AccountConfigCalls {
        fn from(value: ListGroupsCall) -> Self {
            Self::ListGroups(value)
        }
    }
    impl ::core::convert::From<ListPkpsCall> for AccountConfigCalls {
        fn from(value: ListPkpsCall) -> Self {
            Self::ListPkps(value)
        }
    }
    impl ::core::convert::From<ListWalletsInGroupCall> for AccountConfigCalls {
        fn from(value: ListWalletsInGroupCall) -> Self {
            Self::ListWalletsInGroup(value)
        }
    }
    impl ::core::convert::From<NewAccountCall> for AccountConfigCalls {
        fn from(value: NewAccountCall) -> Self {
            Self::NewAccount(value)
        }
    }
    impl ::core::convert::From<PkpCountCall> for AccountConfigCalls {
        fn from(value: PkpCountCall) -> Self {
            Self::PkpCount(value)
        }
    }
    impl ::core::convert::From<PricingAtCall> for AccountConfigCalls {
        fn from(value: PricingAtCall) -> Self {
            Self::PricingAt(value)
        }
    }
    impl ::core::convert::From<PricingOperatorCall> for AccountConfigCalls {
        fn from(value: PricingOperatorCall) -> Self {
            Self::PricingOperator(value)
        }
    }
    impl ::core::convert::From<RebalanceAmountCall> for AccountConfigCalls {
        fn from(value: RebalanceAmountCall) -> Self {
            Self::RebalanceAmount(value)
        }
    }
    impl ::core::convert::From<RegisterWalletDerivationCall> for AccountConfigCalls {
        fn from(value: RegisterWalletDerivationCall) -> Self {
            Self::RegisterWalletDerivation(value)
        }
    }
    impl ::core::convert::From<RemoveActionFromGroupCall> for AccountConfigCalls {
        fn from(value: RemoveActionFromGroupCall) -> Self {
            Self::RemoveActionFromGroup(value)
        }
    }
    impl ::core::convert::From<RemovePkpFromGroupCall> for AccountConfigCalls {
        fn from(value: RemovePkpFromGroupCall) -> Self {
            Self::RemovePkpFromGroup(value)
        }
    }
    impl ::core::convert::From<RemoveUsageApiKeyCall> for AccountConfigCalls {
        fn from(value: RemoveUsageApiKeyCall) -> Self {
            Self::RemoveUsageApiKey(value)
        }
    }
    impl ::core::convert::From<RequestedApiPayerCountCall> for AccountConfigCalls {
        fn from(value: RequestedApiPayerCountCall) -> Self {
            Self::RequestedApiPayerCount(value)
        }
    }
    impl ::core::convert::From<SetAdminApiPayerAccountCall> for AccountConfigCalls {
        fn from(value: SetAdminApiPayerAccountCall) -> Self {
            Self::SetAdminApiPayerAccount(value)
        }
    }
    impl ::core::convert::From<SetApiPayersCall> for AccountConfigCalls {
        fn from(value: SetApiPayersCall) -> Self {
            Self::SetApiPayers(value)
        }
    }
    impl ::core::convert::From<SetPricingCall> for AccountConfigCalls {
        fn from(value: SetPricingCall) -> Self {
            Self::SetPricing(value)
        }
    }
    impl ::core::convert::From<SetPricingOperatorCall> for AccountConfigCalls {
        fn from(value: SetPricingOperatorCall) -> Self {
            Self::SetPricingOperator(value)
        }
    }
    impl ::core::convert::From<SetRebalanceAmountCall> for AccountConfigCalls {
        fn from(value: SetRebalanceAmountCall) -> Self {
            Self::SetRebalanceAmount(value)
        }
    }
    impl ::core::convert::From<SetRequestedApiPayerCountCall> for AccountConfigCalls {
        fn from(value: SetRequestedApiPayerCountCall) -> Self {
            Self::SetRequestedApiPayerCount(value)
        }
    }
    impl ::core::convert::From<SetUsageApiKeyCall> for AccountConfigCalls {
        fn from(value: SetUsageApiKeyCall) -> Self {
            Self::SetUsageApiKey(value)
        }
    }
    impl ::core::convert::From<UpdateActionMetadataCall> for AccountConfigCalls {
        fn from(value: UpdateActionMetadataCall) -> Self {
            Self::UpdateActionMetadata(value)
        }
    }
    impl ::core::convert::From<UpdateGroupCall> for AccountConfigCalls {
        fn from(value: UpdateGroupCall) -> Self {
            Self::UpdateGroup(value)
        }
    }
    impl ::core::convert::From<UpdateUsageApiKeyMetadataCall> for AccountConfigCalls {
        fn from(value: UpdateUsageApiKeyMetadataCall) -> Self {
            Self::UpdateUsageApiKeyMetadata(value)
        }
    }
    ///Container type for all return fields from the `accountCount` function with signature `accountCount()` and selector `0xe4af29fc`
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
    pub struct AccountCountReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `accountExistsAndIsMutable` function with signature `accountExistsAndIsMutable(uint256)` and selector `0x719fac43`
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
    pub struct AccountExistsAndIsMutableReturn(pub bool);
    ///Container type for all return fields from the `adminApiPayerAccount` function with signature `adminApiPayerAccount()` and selector `0x383603fe`
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
    pub struct AdminApiPayerAccountReturn(pub ::ethers::core::types::Address);
    ///Container type for all return fields from the `allPkpIdsAt` function with signature `allPkpIdsAt(uint256)` and selector `0x62feae44`
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
    pub struct AllPkpIdsAtReturn(pub ::ethers::core::types::Address);
    ///Container type for all return fields from the `apiPayerCount` function with signature `apiPayerCount()` and selector `0xb8037ffe`
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
    pub struct ApiPayerCountReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `api_payers` function with signature `api_payers()` and selector `0x93c8bc43`
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
    pub struct ApiPayersReturn(pub ::std::vec::Vec<::ethers::core::types::Address>);
    ///Container type for all return fields from the `getPricing` function with signature `getPricing(uint256)` and selector `0xc12f1a42`
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
    pub struct GetPricingReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `getWalletDerivation` function with signature `getWalletDerivation(uint256,address)` and selector `0x90222cad`
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
    pub struct GetWalletDerivationReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `indexToAccountHashAt` function with signature `indexToAccountHashAt(uint256)` and selector `0x6fe1fb84`
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
    pub struct IndexToAccountHashAtReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `listActions` function with signature `listActions(uint256,uint256,uint256,uint256)` and selector `0x542970ed`
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
    pub struct ListActionsReturn(pub ::std::vec::Vec<Metadata>);
    ///Container type for all return fields from the `listApiKeys` function with signature `listApiKeys(uint256,uint256,uint256)` and selector `0xc704668c`
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
    pub struct ListApiKeysReturn(pub ::std::vec::Vec<UsageApiKeyReturn>);
    ///Container type for all return fields from the `listGroupContents` function with signature `listGroupContents(uint256,uint256)` and selector `0x6fe9572e`
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
    pub struct ListGroupContentsReturn(pub GroupReturn);
    ///Container type for all return fields from the `listGroups` function with signature `listGroups(uint256,uint256,uint256)` and selector `0xf75c8b2d`
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
    pub struct ListGroupsReturn(pub ::std::vec::Vec<Metadata>);
    ///Container type for all return fields from the `listPkps` function with signature `listPkps(uint256,uint256,uint256)` and selector `0x50409001`
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
    pub struct ListPkpsReturn(pub ::std::vec::Vec<PkpData>);
    ///Container type for all return fields from the `listWalletsInGroup` function with signature `listWalletsInGroup(uint256,uint256,uint256,uint256)` and selector `0x291ff1ea`
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
    pub struct ListWalletsInGroupReturn(pub ::std::vec::Vec<PkpData>);
    ///Container type for all return fields from the `pkpCount` function with signature `pkpCount()` and selector `0x223a7962`
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
    pub struct PkpCountReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `pricingAt` function with signature `pricingAt(uint256)` and selector `0xc1aff899`
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
    pub struct PricingAtReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `pricingOperator` function with signature `pricingOperator()` and selector `0xcb53ad26`
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
    pub struct PricingOperatorReturn(pub ::ethers::core::types::Address);
    ///Container type for all return fields from the `rebalanceAmount` function with signature `rebalanceAmount()` and selector `0x3150289b`
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
    pub struct RebalanceAmountReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `requestedApiPayerCount` function with signature `requestedApiPayerCount()` and selector `0x34b7f87a`
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
    pub struct RequestedApiPayerCountReturn(pub ::ethers::core::types::U256);
    ///`Metadata(uint256,string,string)`
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
    pub struct Metadata {
        pub id: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
    ///`PkpData(uint256,address,string,string)`
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
    pub struct PkpData {
        pub id: ::ethers::core::types::U256,
        pub pkp_id: ::ethers::core::types::Address,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
    ///`GroupReturn((uint256,string,string),uint256[],address[])`
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
    pub struct GroupReturn {
        pub metadata: Metadata,
        pub cid_hash: ::std::vec::Vec<::ethers::core::types::U256>,
        pub pkp_id: ::std::vec::Vec<::ethers::core::types::Address>,
    }
    ///`UsageApiKeyReturn((uint256,string,string),uint256,uint256,uint256,uint256[],bool,bool,bool,uint256[],uint256[],uint256[])`
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
    pub struct UsageApiKeyReturn {
        pub metadata: Metadata,
        pub api_key_hash: ::ethers::core::types::U256,
        pub expiration: ::ethers::core::types::U256,
        pub balance: ::ethers::core::types::U256,
        pub execute_in_groups: ::std::vec::Vec<::ethers::core::types::U256>,
        pub create_groups: bool,
        pub delete_groups: bool,
        pub create_pk_ps: bool,
        pub manage_ipfs_ids_in_groups: ::std::vec::Vec<::ethers::core::types::U256>,
        pub add_pkp_to_groups: ::std::vec::Vec<::ethers::core::types::U256>,
        pub remove_pkp_from_groups: ::std::vec::Vec<::ethers::core::types::U256>,
    }
}
