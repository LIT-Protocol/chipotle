pub use account_config::*;
/// This module was auto-generated with ethers-rs Abigen.
/// More information at: <https://github.com/gakonst/ethers-rs>
#[allow(
    clippy::enum_variant_names,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms,
    clippy::type_complexity,
    dead_code,
    non_camel_case_types,
)]
pub mod account_config {
    const _: () = {
        ::core::include_bytes!(
            "./AccountConfig.json",
        );
    };
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::Some(::ethers::core::abi::ethabi::Constructor {
                inputs: ::std::vec![],
            }),
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("accountExistsAndIsMutable"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "accountExistsAndIsMutable",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bool"),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addActionToGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("addActionToGroup"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("action"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
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
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addApiKey"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("addApiKey"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("usageApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("expiration"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("balance"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("addGroup"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
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
                                    name: ::std::borrow::ToOwned::to_owned("permitted_actions"),
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
                                    name: ::std::borrow::ToOwned::to_owned("Wallets"),
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
                                    name: ::std::borrow::ToOwned::to_owned(
                                        "all_wallets_permitted",
                                    ),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bool"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned(
                                        "all_actions_permitted",
                                    ),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bool"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addWalletToGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("addWalletToGroup"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("walletAddressHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getWalletDerivation"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "getWalletDerivation",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("walletAddressHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listActions"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("listActions"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct AccountConfig.Metadata[]",
                                        ),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listApiKeys"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("listApiKeys"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                        ::std::vec![
                                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                            ::ethers::core::abi::ethabi::ParamType::String,
                                                            ::ethers::core::abi::ethabi::ParamType::String,
                                                        ],
                                                    ),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct AccountConfig.UsageApiKey[]",
                                        ),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listGroups"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("listGroups"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct AccountConfig.Metadata[]",
                                        ),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listWallets"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("listWallets"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct AccountConfig.Metadata[]",
                                        ),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("listWalletsInGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("listWalletsInGroup"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageNumber"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("pageSize"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                    ::ethers::core::abi::ethabi::ParamType::String,
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct AccountConfig.Metadata[]",
                                        ),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("newAccount"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("newAccount"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("apiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
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
                                    name: ::std::borrow::ToOwned::to_owned(
                                        "accountDescription",
                                    ),
                                    kind: ::ethers::core::abi::ethabi::ParamType::String,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("string"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned(
                                        "creatorWalletAddress",
                                    ),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("initialBalance"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("nextWalletCount"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("nextWalletCount"),
                            inputs: ::std::vec![],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("owner"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("owner"),
                            inputs: ::std::vec![],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("registerWalletDerivation"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "registerWalletDerivation",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("walletAddressHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("derivationPath"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
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
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("removeActionFromGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "removeActionFromGroup",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("action"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("removeUsageApiKey"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("removeUsageApiKey"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("usageApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("removeWalletFromGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "removeWalletFromGroup",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("walletAddressHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("updateActionMetadata"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "updateActionMetadata",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("actionHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
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
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("updateGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("updateGroup"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
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
                                    name: ::std::borrow::ToOwned::to_owned(
                                        "all_wallets_permitted",
                                    ),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bool"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned(
                                        "all_actions_permitted",
                                    ),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bool"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("updateUsageApiKeyMetadata"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "updateUsageApiKeyMetadata",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("usageApiKeyHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
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
                        },
                    ],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("AccountDoesNotExist"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "AccountDoesNotExist",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("apiKey"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("ActionDoesNotExist"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("ActionDoesNotExist"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("apiKey"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("action"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("GroupDoesNotExist"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("GroupDoesNotExist"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("apiKey"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("UsageApiKeyDoesNotExist"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "UsageApiKeyDoesNotExist",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("apiKey"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("usageApiKey"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("WalletDoesNotExist"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("WalletDoesNotExist"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("apiKey"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("groupId"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("Wallet"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
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
    pub static ACCOUNTCONFIG_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> = ::ethers::contract::Lazy::new(
        __abi,
    );
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80`@R4\x80\x15`\x0EW__\xFD[P3`\x02_a\x01\0\n\x81T\x81s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x02\x19\x16\x90\x83s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x02\x17\x90UP`\x01`\x03\x81\x90UPa/1\x80a\0d_9_\xF3\xFE`\x80`@R4\x80\x15a\0\x0FW__\xFD[P`\x046\x10a\x015W_5`\xE0\x1C\x80cy\xB8\xE6\x91\x11a\0\xB6W\x80c\xBA\xC7\x10\xEA\x11a\0zW\x80c\xBA\xC7\x10\xEA\x14a\x03EW\x80c\xBD\x9A\xEDQ\x14a\x03aW\x80c\xC5\xF5\xB9\x84\x14a\x03}W\x80c\xC7\x04f\x8C\x14a\x03\x99W\x80c\xDB\xB1z\x0B\x14a\x03\xC9W\x80c\xF7\\\x8B-\x14a\x03\xE5Wa\x015V[\x80cy\xB8\xE6\x91\x14a\x02\x8FW\x80cz\xF3a\xEF\x14a\x02\xBFW\x80c\x8D\xA5\xCB[\x14a\x02\xEFW\x80c\x96\xA7\xCCT\x14a\x03\rW\x80c\xA6\xB6\xB6r\x14a\x03)Wa\x015V[\x80c]\x86\x1Cr\x11a\0\xFDW\x80c]\x86\x1Cr\x14a\x01\xEFW\x80cj=w\xA9\x14a\x02\x0BW\x80cn\x06\xAC\x9C\x14a\x02'W\x80cq\x9F\xACC\x14a\x02CW\x80ct\x9EM\x07\x14a\x02sWa\x015V[\x80c)\x1F\xF1\xEA\x14a\x019W\x80c/\xA9.A\x14a\x01iW\x80cG\x05\x16\x1E\x14a\x01\x85W\x80cIqy5\x14a\x01\xA3W\x80cT)p\xED\x14a\x01\xBFW[__\xFD[a\x01S`\x04\x806\x03\x81\x01\x90a\x01N\x91\x90a\x1F`V[a\x04\x15V[`@Qa\x01`\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[a\x01\x83`\x04\x806\x03\x81\x01\x90a\x01~\x91\x90a\"\x9EV[a\x06\xC6V[\0[a\x01\x8Da\x07\x8FV[`@Qa\x01\x9A\x91\x90a#\\V[`@Q\x80\x91\x03\x90\xF3[a\x01\xBD`\x04\x806\x03\x81\x01\x90a\x01\xB8\x91\x90a\x1F`V[a\x07\x95V[\0[a\x01\xD9`\x04\x806\x03\x81\x01\x90a\x01\xD4\x91\x90a\x1F`V[a\x08\x0BV[`@Qa\x01\xE6\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[a\x02\t`\x04\x806\x03\x81\x01\x90a\x02\x04\x91\x90a#uV[a\n\xBCV[\0[a\x02%`\x04\x806\x03\x81\x01\x90a\x02 \x91\x90a#\xC5V[a\x0B/V[\0[a\x02A`\x04\x806\x03\x81\x01\x90a\x02<\x91\x90a#uV[a\x0B\x9FV[\0[a\x02]`\x04\x806\x03\x81\x01\x90a\x02X\x91\x90a$aV[a\x0B\xECV[`@Qa\x02j\x91\x90a$\xA6V[`@Q\x80\x91\x03\x90\xF3[a\x02\x8D`\x04\x806\x03\x81\x01\x90a\x02\x88\x91\x90a%\xADV[a\x0C\xEFV[\0[a\x02\xA9`\x04\x806\x03\x81\x01\x90a\x02\xA4\x91\x90a&\xBAV[a\x0EkV[`@Qa\x02\xB6\x91\x90a#\\V[`@Q\x80\x91\x03\x90\xF3[a\x02\xD9`\x04\x806\x03\x81\x01\x90a\x02\xD4\x91\x90a#uV[a\x0E\xA5V[`@Qa\x02\xE6\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[a\x02\xF7a\x11\x15V[`@Qa\x03\x04\x91\x90a'7V[`@Q\x80\x91\x03\x90\xF3[a\x03'`\x04\x806\x03\x81\x01\x90a\x03\"\x91\x90a'PV[a\x11:V[\0[a\x03C`\x04\x806\x03\x81\x01\x90a\x03>\x91\x90a\"\x9EV[a\x12\x07V[\0[a\x03_`\x04\x806\x03\x81\x01\x90a\x03Z\x91\x90a#uV[a\x12uV[\0[a\x03{`\x04\x806\x03\x81\x01\x90a\x03v\x91\x90a(;V[a\x12\xC1V[\0[a\x03\x97`\x04\x806\x03\x81\x01\x90a\x03\x92\x91\x90a&\xBAV[a\x13\x9FV[\0[a\x03\xB3`\x04\x806\x03\x81\x01\x90a\x03\xAE\x91\x90a#uV[a\x13\xD8V[`@Qa\x03\xC0\x91\x90a*\x17V[`@Q\x80\x91\x03\x90\xF3[a\x03\xE3`\x04\x806\x03\x81\x01\x90a\x03\xDE\x91\x90a\"\x9EV[a\x16\x80V[\0[a\x03\xFF`\x04\x806\x03\x81\x01\x90a\x03\xFA\x91\x90a#uV[a\x17fV[`@Qa\x04\x0C\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[``a\x04!\x85\x85a\x1A\0V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P_\x81`\x0F\x01_\x87\x81R` \x01\x90\x81R` \x01_ \x90Pa\x04V\x81`\x05\x01a\x1AQV[\x84\x11\x15a\x04oWa\x04i\x81`\x05\x01a\x1AQV[\x93P_\x94P[_\x84\x86a\x04|\x91\x90a*dV[\x90P_\x85\x82a\x04\x8B\x91\x90a*\xA5V[\x90Pa\x04\x99\x83`\x05\x01a\x1AQV[\x81\x11\x15a\x04\xAFWa\x04\xAC\x83`\x05\x01a\x1AQV[\x90P[_\x82\x82a\x04\xBC\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x04\xD9Wa\x04\xD8a!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x05\x12W\x81` \x01[a\x04\xFFa\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x04\xF7W\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\x06\xB4W\x86`\x15\x01_a\x05H\x83\x88a\x056\x91\x90a*\xA5V[\x89`\x05\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ `@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x05y\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x05\xA5\x90a+8V[\x80\x15a\x05\xF0W\x80`\x1F\x10a\x05\xC7Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x05\xF0V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x05\xD3W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x06\t\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x065\x90a+8V[\x80\x15a\x06\x80W\x80`\x1F\x10a\x06WWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x06\x80V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x06cW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\x06\x9CWa\x06\x9Ba+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x05\x1AV[P\x80\x96PPPPPPP\x94\x93PPPPV[a\x06\xD0\x85\x85a\x1A\0V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90Pa\x07\x0B\x84\x82`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x83\x81`\x12\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x01\x81\x90UP\x82\x81`\x12\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01\x90\x81a\x07I\x91\x90a-5V[P\x81\x81`\x12\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x02\x01\x90\x81a\x07m\x91\x90a-5V[P\x80`\x17\x01_\x81T\x80\x92\x91\x90a\x07\x82\x90a.\x04V[\x91\x90PUPPPPPPPV[`\x03T\x81V[a\x07\x9E\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ `\x0C\x01_\x85\x81R` \x01\x90\x81R` \x01_ \x90P\x83\x81`\x03\x01\x81\x90UP\x82\x81`\x04\x01\x81\x90UP\x81\x81`\x05\x01\x81\x90UPa\x08\x03\x84__\x88\x81R` \x01\x90\x81R` \x01_ `\n\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPPV[``a\x08\x17\x85\x85a\x1A\0V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P_\x81`\x0F\x01_\x87\x81R` \x01\x90\x81R` \x01_ \x90Pa\x08L\x81`\x03\x01a\x1AQV[\x84\x11\x15a\x08eWa\x08_\x81`\x03\x01a\x1AQV[\x93P_\x94P[_\x84\x86a\x08r\x91\x90a*dV[\x90P_\x85\x82a\x08\x81\x91\x90a*\xA5V[\x90Pa\x08\x8F\x83`\x03\x01a\x1AQV[\x81\x11\x15a\x08\xA5Wa\x08\xA2\x83`\x03\x01a\x1AQV[\x90P[_\x82\x82a\x08\xB2\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x08\xCFWa\x08\xCEa!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\t\x08W\x81` \x01[a\x08\xF5a\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x08\xEDW\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\n\xAAW\x86`\x12\x01_a\t>\x83\x88a\t,\x91\x90a*\xA5V[\x89`\x03\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ `@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\to\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\t\x9B\x90a+8V[\x80\x15a\t\xE6W\x80`\x1F\x10a\t\xBDWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\t\xE6V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\t\xC9W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\t\xFF\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\n+\x90a+8V[\x80\x15a\nvW\x80`\x1F\x10a\nMWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\nvV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\nYW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\n\x92Wa\n\x91a+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\t\x10V[P\x80\x96PPPPPPP\x94\x93PPPPV[a\n\xC7\x83\x83\x83a\x1A\xDFV[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x0B\x02\x82\x82`\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x1B4\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P_\x81`\x17\x01T\x11\x15a\x0B)W\x80`\x17\x01_\x81T\x80\x92\x91\x90a\x0B#\x90a.KV[\x91\x90PUP[PPPPV[a\x0B9\x84\x84a\x1BKV[___\x86\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81`\x0C\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x01`\x01\x01\x90\x81a\x0Bq\x91\x90a-5V[P\x81\x81`\x0C\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x01`\x02\x01\x90\x81a\x0B\x97\x91\x90a-5V[PPPPPPV[a\x0B\xAA\x83\x83\x83a\x1B\x9CV[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x0B\xE5\x82\x82`\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x05\x01a\x1B4\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[____\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81_\x01_\x01T\x14a\x0C\x15W_\x91PPa\x0C\xEAV[`\x02_\x90T\x90a\x01\0\n\x90\x04s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x163s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x14\x80\x15a\x0C\x85WP`\x01\x15\x15\x81`\x13\x01_\x90T\x90a\x01\0\n\x90\x04`\xFF\x16\x15\x15\x14[\x15a\x0C\x94W`\x01\x91PPa\x0C\xEAV[3s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x81`\t\x01_\x90T\x90a\x01\0\n\x90\x04s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x14\x91PP[\x91\x90PV[a\x0C\xF8\x87a\x1A\x92V[___\x89\x81R` \x01\x90\x81R` \x01_ \x90Pa\r%\x81`\x18\x01T\x82`\r\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P_\x81`\x0F\x01_\x83`\x18\x01T\x81R` \x01\x90\x81R` \x01_ \x90P\x81`\x18\x01T\x81_\x01_\x01\x81\x90UP\x87\x81_\x01`\x01\x01\x90\x81a\ra\x91\x90a-5V[P\x86\x81_\x01`\x02\x01\x90\x81a\ru\x91\x90a-5V[P__\x90P[\x86Q\x81\x10\x15a\r\xC2Wa\r\xB4\x87\x82\x81Q\x81\x10a\r\x9AWa\r\x99a+hV[[` \x02` \x01\x01Q\x83`\x03\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\r{V[P__\x90P[\x85Q\x81\x10\x15a\x0E\x0FWa\x0E\x01\x86\x82\x81Q\x81\x10a\r\xE7Wa\r\xE6a+hV[[` \x02` \x01\x01Q\x83`\x05\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\r\xC8V[P\x83\x81`\x07\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x82\x81`\x07\x01`\x01a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x81`\x18\x01_\x81T\x80\x92\x91\x90a\x0E[\x90a.\x04V[\x91\x90PUPPPPPPPPPPV[_a\x0Eu\x83a\x1A\x92V[___\x85\x81R` \x01\x90\x81R` \x01_ \x90P\x80`\x14\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x91PP\x92\x91PPV[``a\x0E\xB0\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ \x90P`\x03T\x83\x11\x15a\x0E\xD6W`\x03T\x92P_\x93P[_\x83\x85a\x0E\xE3\x91\x90a*dV[\x90P_\x84\x82a\x0E\xF2\x91\x90a*\xA5V[\x90P`\x03T\x81\x11\x15a\x0F\x04W`\x03T\x90P[_\x82\x82a\x0F\x11\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x0F.Wa\x0F-a!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x0FgW\x81` \x01[a\x0FTa\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x0FLW\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\x11\x05W\x85`\x15\x01_`\x01_\x84\x89a\x0F\x8B\x91\x90a*\xA5V[\x81R` \x01\x90\x81R` \x01_ T\x81R` \x01\x90\x81R` \x01_ `@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x0F\xCA\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x0F\xF6\x90a+8V[\x80\x15a\x10AW\x80`\x1F\x10a\x10\x18Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x10AV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x10$W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x10Z\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x10\x86\x90a+8V[\x80\x15a\x10\xD1W\x80`\x1F\x10a\x10\xA8Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x10\xD1V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x10\xB4W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\x10\xEDWa\x10\xECa+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x0FoV[P\x80\x95PPPPPP\x93\x92PPPV[`\x02_\x90T\x90a\x01\0\n\x90\x04s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x81V[a\x11D\x86\x86a\x1A\0V[___\x88\x81R` \x01\x90\x81R` \x01_ \x90P\x84\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ _\x01`\x01\x01\x90\x81a\x11|\x91\x90a-5V[P\x83\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ _\x01`\x02\x01\x90\x81a\x11\xA2\x91\x90a-5V[P\x82\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ `\x07\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x81\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ `\x07\x01`\x01a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UPPPPPPPPV[a\x12\x12\x85\x84\x86a\x1A\xDFV[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81`\x12\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x01\x01\x90\x81a\x12H\x91\x90a-5V[P\x81\x81`\x12\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x02\x01\x90\x81a\x12l\x91\x90a-5V[PPPPPPPV[a\x12\x7F\x83\x83a\x1A\0V[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x12\xBA\x82\x82`\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x05\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[___\x88\x81R` \x01\x90\x81R` \x01_ \x90P\x85\x81`\x13\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x82\x81`\t\x01_a\x01\0\n\x81T\x81s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x02\x19\x16\x90\x83s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x02\x17\x90UP\x86\x81_\x01_\x01\x81\x90UP\x84\x81_\x01`\x01\x01\x90\x81a\x13N\x91\x90a-5V[P\x83\x81_\x01`\x02\x01\x90\x81a\x13b\x91\x90a-5V[P\x86\x81`\x03\x01`\x03\x01\x81\x90UPc\x12\xCC\x03\0Ba\x13\x7F\x91\x90a*\xA5V[\x81`\x03\x01`\x04\x01\x81\x90UP\x81\x81`\x03\x01`\x05\x01\x81\x90UPPPPPPPPV[a\x13\xA9\x82\x82a\x1BKV[___\x84\x81R` \x01\x90\x81R` \x01_ \x90Pa\x13\xD2\x82\x82`\n\x01a\x1B4\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPV[``a\x13\xE3\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ \x90P_a\x14\x03\x82`\n\x01a\x1AQV[\x90P\x80\x84\x11\x15a\x14\x14W\x80\x93P_\x94P[_\x84\x86a\x14!\x91\x90a*dV[\x90P_\x85\x82a\x140\x91\x90a*\xA5V[\x90P\x82\x81\x11\x15a\x14>W\x82\x90P[_\x82\x82a\x14K\x91\x90a*\xD8V[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x14dWa\x14ca!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x14\x9DW\x81` \x01[a\x14\x8Aa\x1E\xF1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x14\x82W\x90P[P\x90P__\x90P[\x84\x81\x10\x15a\x16pW\x85`\x0C\x01_a\x14\xD3\x83\x87a\x14\xC1\x91\x90a*\xA5V[\x89`\n\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ `@Q\x80`\x80\x01`@R\x90\x81_\x82\x01`@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x15\x13\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x15?\x90a+8V[\x80\x15a\x15\x8AW\x80`\x1F\x10a\x15aWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x15\x8AV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x15mW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x15\xA3\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x15\xCF\x90a+8V[\x80\x15a\x16\x1AW\x80`\x1F\x10a\x15\xF1Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x16\x1AV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x15\xFDW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x81R` \x01`\x03\x82\x01T\x81R` \x01`\x04\x82\x01T\x81R` \x01`\x05\x82\x01T\x81RPP\x82\x82\x81Q\x81\x10a\x16XWa\x16Wa+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x14\xA5V[P\x80\x95PPPPPP\x93\x92PPPV[a\x16\x89\x85a\x1A\x92V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P\x83\x81`\x14\x01_\x87\x81R` \x01\x90\x81R` \x01_ \x81\x90UP\x84\x81`\x15\x01_\x87\x81R` \x01\x90\x81R` \x01_ _\x01\x81\x90UP\x82\x81`\x15\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x01\x01\x90\x81a\x16\xF1\x91\x90a-5V[P\x81\x81`\x15\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x02\x01\x90\x81a\x17\x15\x91\x90a-5V[P\x84`\x01_`\x03T\x81R` \x01\x90\x81R` \x01_ \x81\x90UP\x80`\x16\x01_\x81T\x80\x92\x91\x90a\x17B\x90a.\x04V[\x91\x90PUP`\x03_\x81T\x80\x92\x91\x90a\x17Y\x90a.\x04V[\x91\x90PUPPPPPPPV[``a\x17q\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ \x90Pa\x17\x90\x81`\r\x01a\x1AQV[\x83\x11\x15a\x17\xA9Wa\x17\xA3\x81`\r\x01a\x1AQV[\x92P_\x93P[_\x83\x85a\x17\xB6\x91\x90a*dV[\x90P_\x84\x82a\x17\xC5\x91\x90a*\xA5V[\x90Pa\x17\xD3\x83`\r\x01a\x1AQV[\x81\x11\x15a\x17\xE9Wa\x17\xE6\x83`\r\x01a\x1AQV[\x90P[_\x82\x82a\x17\xF6\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x18\x13Wa\x18\x12a!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x18LW\x81` \x01[a\x189a\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x181W\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\x19\xF0W\x85`\x0F\x01_a\x18\x82\x83\x88a\x18p\x91\x90a*\xA5V[\x89`\r\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ _\x01`@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x18\xB5\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x18\xE1\x90a+8V[\x80\x15a\x19,W\x80`\x1F\x10a\x19\x03Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x19,V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x19\x0FW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x19E\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x19q\x90a+8V[\x80\x15a\x19\xBCW\x80`\x1F\x10a\x19\x93Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x19\xBCV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x19\x9FW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\x19\xD8Wa\x19\xD7a+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x18TV[P\x80\x95PPPPPP\x93\x92PPPV[a\x1A\n\x82\x82a\x1B\xF1V[a\x1AMW\x81\x81`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1AD\x92\x91\x90a.rV[`@Q\x80\x91\x03\x90\xFD[PPV[_a\x1A]\x82_\x01a\x1C1V[\x90P\x91\x90PV[_a\x1Aq\x83_\x01\x83a\x1C@V[_\x1C\x90P\x92\x91PPV[_a\x1A\x8A\x83_\x01\x83_\x1Ba\x1CgV[\x90P\x92\x91PPV[a\x1A\x9B\x81a\x0B\xECV[a\x1A\xDCW\x80`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1A\xD3\x91\x90a#\\V[`@Q\x80\x91\x03\x90\xFD[PV[a\x1A\xEA\x83\x83\x83a\x1C\xCEV[a\x1B/W\x82\x82\x82`@Q\x7F\xEF%\xD0-\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1B&\x93\x92\x91\x90a.\x99V[`@Q\x80\x91\x03\x90\xFD[PPPV[_a\x1BC\x83_\x01\x83_\x1Ba\x1D\x19V[\x90P\x92\x91PPV[a\x1BU\x82\x82a\x1E\x15V[a\x1B\x98W\x81\x81`@Q\x7Ft\x8Eq*\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1B\x8F\x92\x91\x90a.rV[`@Q\x80\x91\x03\x90\xFD[PPV[a\x1B\xA7\x83\x83\x83a\x1EOV[a\x1B\xECW\x82\x82\x82`@Q\x7Fy\x16xX\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1B\xE3\x93\x92\x91\x90a.\x99V[`@Q\x80\x91\x03\x90\xFD[PPPV[_a\x1B\xFB\x83a\x1A\x92V[___\x85\x81R` \x01\x90\x81R` \x01_ `\x0F\x01_\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81_\x01_\x01T\x14\x91PP\x92\x91PPV[_\x81_\x01\x80T\x90P\x90P\x91\x90PV[_\x82_\x01\x82\x81T\x81\x10a\x1CVWa\x1CUa+hV[[\x90_R` _ \x01T\x90P\x92\x91PPV[_a\x1Cr\x83\x83a\x1E\x9AV[a\x1C\xC4W\x82_\x01\x82\x90\x80`\x01\x81T\x01\x80\x82U\x80\x91PP`\x01\x90\x03\x90_R` _ \x01_\x90\x91\x90\x91\x90\x91PU\x82_\x01\x80T\x90P\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ \x81\x90UP`\x01\x90Pa\x1C\xC8V[_\x90P[\x92\x91PPV[_a\x1C\xD9\x84\x84a\x1A\0V[a\x1D\x10\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x1E\xBA\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x90P_\x81\x14a\x1E\nW_`\x01\x82a\x1DF\x91\x90a*\xD8V[\x90P_`\x01\x86_\x01\x80T\x90Pa\x1D\\\x91\x90a*\xD8V[\x90P\x80\x82\x14a\x1D\xC2W_\x86_\x01\x82\x81T\x81\x10a\x1D{Wa\x1Dza+hV[[\x90_R` _ \x01T\x90P\x80\x87_\x01\x84\x81T\x81\x10a\x1D\x9CWa\x1D\x9Ba+hV[[\x90_R` _ \x01\x81\x90UP\x83\x87`\x01\x01_\x83\x81R` \x01\x90\x81R` \x01_ \x81\x90UPP[\x85_\x01\x80T\x80a\x1D\xD5Wa\x1D\xD4a.\xCEV[[`\x01\x90\x03\x81\x81\x90_R` _ \x01_\x90U\x90U\x85`\x01\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x90U`\x01\x93PPPPa\x1E\x0FV[_\x91PP[\x92\x91PPV[_a\x1E\x1F\x83a\x1A\x92V[\x81__\x85\x81R` \x01\x90\x81R` \x01_ `\x0C\x01_\x84\x81R` \x01\x90\x81R` \x01_ `\x03\x01T\x14\x90P\x92\x91PPV[_a\x1EZ\x84\x84a\x1A\0V[a\x1E\x91\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x05\x01a\x1E\xBA\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x14\x15\x90P\x92\x91PPV[_a\x1E\xC9\x83_\x01\x83_\x1Ba\x1E\x9AV[\x90P\x92\x91PPV[`@Q\x80``\x01`@R\x80_\x81R` \x01``\x81R` \x01``\x81RP\x90V[`@Q\x80`\x80\x01`@R\x80a\x1F\x04a\x1E\xD1V[\x81R` \x01_\x81R` \x01_\x81R` \x01_\x81RP\x90V[_`@Q\x90P\x90V[__\xFD[__\xFD[_\x81\x90P\x91\x90PV[a\x1F?\x81a\x1F-V[\x81\x14a\x1FIW__\xFD[PV[_\x815\x90Pa\x1FZ\x81a\x1F6V[\x92\x91PPV[____`\x80\x85\x87\x03\x12\x15a\x1FxWa\x1Fwa\x1F%V[[_a\x1F\x85\x87\x82\x88\x01a\x1FLV[\x94PP` a\x1F\x96\x87\x82\x88\x01a\x1FLV[\x93PP`@a\x1F\xA7\x87\x82\x88\x01a\x1FLV[\x92PP``a\x1F\xB8\x87\x82\x88\x01a\x1FLV[\x91PP\x92\x95\x91\x94P\x92PV[_\x81Q\x90P\x91\x90PV[_\x82\x82R` \x82\x01\x90P\x92\x91PPV[_\x81\x90P` \x82\x01\x90P\x91\x90PV[a\x1F\xF6\x81a\x1F-V[\x82RPPV[_\x81Q\x90P\x91\x90PV[_\x82\x82R` \x82\x01\x90P\x92\x91PPV[\x82\x81\x83^_\x83\x83\x01RPPPV[_`\x1F\x19`\x1F\x83\x01\x16\x90P\x91\x90PV[_a >\x82a\x1F\xFCV[a H\x81\x85a \x06V[\x93Pa X\x81\x85` \x86\x01a \x16V[a a\x81a $V[\x84\x01\x91PP\x92\x91PPV[_``\x83\x01_\x83\x01Qa \x81_\x86\x01\x82a\x1F\xEDV[P` \x83\x01Q\x84\x82\x03` \x86\x01Ra \x99\x82\x82a 4V[\x91PP`@\x83\x01Q\x84\x82\x03`@\x86\x01Ra \xB3\x82\x82a 4V[\x91PP\x80\x91PP\x92\x91PPV[_a \xCB\x83\x83a lV[\x90P\x92\x91PPV[_` \x82\x01\x90P\x91\x90PV[_a \xE9\x82a\x1F\xC4V[a \xF3\x81\x85a\x1F\xCEV[\x93P\x83` \x82\x02\x85\x01a!\x05\x85a\x1F\xDEV[\x80_[\x85\x81\x10\x15a!@W\x84\x84\x03\x89R\x81Qa!!\x85\x82a \xC0V[\x94Pa!,\x83a \xD3V[\x92P` \x8A\x01\x99PP`\x01\x81\x01\x90Pa!\x08V[P\x82\x97P\x87\x95PPPPPP\x92\x91PPV[_` \x82\x01\x90P\x81\x81\x03_\x83\x01Ra!j\x81\x84a \xDFV[\x90P\x92\x91PPV[__\xFD[__\xFD[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`A`\x04R`$_\xFD[a!\xB0\x82a $V[\x81\x01\x81\x81\x10g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x17\x15a!\xCFWa!\xCEa!zV[[\x80`@RPPPV[_a!\xE1a\x1F\x1CV[\x90Pa!\xED\x82\x82a!\xA7V[\x91\x90PV[_g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x15a\"\x0CWa\"\x0Ba!zV[[a\"\x15\x82a $V[\x90P` \x81\x01\x90P\x91\x90PV[\x82\x81\x837_\x83\x83\x01RPPPV[_a\"Ba\"=\x84a!\xF2V[a!\xD8V[\x90P\x82\x81R` \x81\x01\x84\x84\x84\x01\x11\x15a\"^Wa\"]a!vV[[a\"i\x84\x82\x85a\"\"V[P\x93\x92PPPV[_\x82`\x1F\x83\x01\x12a\"\x85Wa\"\x84a!rV[[\x815a\"\x95\x84\x82` \x86\x01a\"0V[\x91PP\x92\x91PPV[_____`\xA0\x86\x88\x03\x12\x15a\"\xB7Wa\"\xB6a\x1F%V[[_a\"\xC4\x88\x82\x89\x01a\x1FLV[\x95PP` a\"\xD5\x88\x82\x89\x01a\x1FLV[\x94PP`@a\"\xE6\x88\x82\x89\x01a\x1FLV[\x93PP``\x86\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a#\x07Wa#\x06a\x1F)V[[a#\x13\x88\x82\x89\x01a\"qV[\x92PP`\x80\x86\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a#4Wa#3a\x1F)V[[a#@\x88\x82\x89\x01a\"qV[\x91PP\x92\x95P\x92\x95\x90\x93PV[a#V\x81a\x1F-V[\x82RPPV[_` \x82\x01\x90Pa#o_\x83\x01\x84a#MV[\x92\x91PPV[___``\x84\x86\x03\x12\x15a#\x8CWa#\x8Ba\x1F%V[[_a#\x99\x86\x82\x87\x01a\x1FLV[\x93PP` a#\xAA\x86\x82\x87\x01a\x1FLV[\x92PP`@a#\xBB\x86\x82\x87\x01a\x1FLV[\x91PP\x92P\x92P\x92V[____`\x80\x85\x87\x03\x12\x15a#\xDDWa#\xDCa\x1F%V[[_a#\xEA\x87\x82\x88\x01a\x1FLV[\x94PP` a#\xFB\x87\x82\x88\x01a\x1FLV[\x93PP`@\x85\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a$\x1CWa$\x1Ba\x1F)V[[a$(\x87\x82\x88\x01a\"qV[\x92PP``\x85\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a$IWa$Ha\x1F)V[[a$U\x87\x82\x88\x01a\"qV[\x91PP\x92\x95\x91\x94P\x92PV[_` \x82\x84\x03\x12\x15a$vWa$ua\x1F%V[[_a$\x83\x84\x82\x85\x01a\x1FLV[\x91PP\x92\x91PPV[_\x81\x15\x15\x90P\x91\x90PV[a$\xA0\x81a$\x8CV[\x82RPPV[_` \x82\x01\x90Pa$\xB9_\x83\x01\x84a$\x97V[\x92\x91PPV[_g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x15a$\xD9Wa$\xD8a!zV[[` \x82\x02\x90P` \x81\x01\x90P\x91\x90PV[__\xFD[_a%\0a$\xFB\x84a$\xBFV[a!\xD8V[\x90P\x80\x83\x82R` \x82\x01\x90P` \x84\x02\x83\x01\x85\x81\x11\x15a%#Wa%\"a$\xEAV[[\x83[\x81\x81\x10\x15a%LW\x80a%8\x88\x82a\x1FLV[\x84R` \x84\x01\x93PP` \x81\x01\x90Pa%%V[PPP\x93\x92PPPV[_\x82`\x1F\x83\x01\x12a%jWa%ia!rV[[\x815a%z\x84\x82` \x86\x01a$\xEEV[\x91PP\x92\x91PPV[a%\x8C\x81a$\x8CV[\x81\x14a%\x96W__\xFD[PV[_\x815\x90Pa%\xA7\x81a%\x83V[\x92\x91PPV[_______`\xE0\x88\x8A\x03\x12\x15a%\xC8Wa%\xC7a\x1F%V[[_a%\xD5\x8A\x82\x8B\x01a\x1FLV[\x97PP` \x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a%\xF6Wa%\xF5a\x1F)V[[a&\x02\x8A\x82\x8B\x01a\"qV[\x96PP`@\x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a&#Wa&\"a\x1F)V[[a&/\x8A\x82\x8B\x01a\"qV[\x95PP``\x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a&PWa&Oa\x1F)V[[a&\\\x8A\x82\x8B\x01a%VV[\x94PP`\x80\x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a&}Wa&|a\x1F)V[[a&\x89\x8A\x82\x8B\x01a%VV[\x93PP`\xA0a&\x9A\x8A\x82\x8B\x01a%\x99V[\x92PP`\xC0a&\xAB\x8A\x82\x8B\x01a%\x99V[\x91PP\x92\x95\x98\x91\x94\x97P\x92\x95PV[__`@\x83\x85\x03\x12\x15a&\xD0Wa&\xCFa\x1F%V[[_a&\xDD\x85\x82\x86\x01a\x1FLV[\x92PP` a&\xEE\x85\x82\x86\x01a\x1FLV[\x91PP\x92P\x92\x90PV[_s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x16\x90P\x91\x90PV[_a'!\x82a&\xF8V[\x90P\x91\x90PV[a'1\x81a'\x17V[\x82RPPV[_` \x82\x01\x90Pa'J_\x83\x01\x84a'(V[\x92\x91PPV[______`\xC0\x87\x89\x03\x12\x15a'jWa'ia\x1F%V[[_a'w\x89\x82\x8A\x01a\x1FLV[\x96PP` a'\x88\x89\x82\x8A\x01a\x1FLV[\x95PP`@\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a'\xA9Wa'\xA8a\x1F)V[[a'\xB5\x89\x82\x8A\x01a\"qV[\x94PP``\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a'\xD6Wa'\xD5a\x1F)V[[a'\xE2\x89\x82\x8A\x01a\"qV[\x93PP`\x80a'\xF3\x89\x82\x8A\x01a%\x99V[\x92PP`\xA0a(\x04\x89\x82\x8A\x01a%\x99V[\x91PP\x92\x95P\x92\x95P\x92\x95V[a(\x1A\x81a'\x17V[\x81\x14a($W__\xFD[PV[_\x815\x90Pa(5\x81a(\x11V[\x92\x91PPV[______`\xC0\x87\x89\x03\x12\x15a(UWa(Ta\x1F%V[[_a(b\x89\x82\x8A\x01a\x1FLV[\x96PP` a(s\x89\x82\x8A\x01a%\x99V[\x95PP`@\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a(\x94Wa(\x93a\x1F)V[[a(\xA0\x89\x82\x8A\x01a\"qV[\x94PP``\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a(\xC1Wa(\xC0a\x1F)V[[a(\xCD\x89\x82\x8A\x01a\"qV[\x93PP`\x80a(\xDE\x89\x82\x8A\x01a('V[\x92PP`\xA0a(\xEF\x89\x82\x8A\x01a\x1FLV[\x91PP\x92\x95P\x92\x95P\x92\x95V[_\x81Q\x90P\x91\x90PV[_\x82\x82R` \x82\x01\x90P\x92\x91PPV[_\x81\x90P` \x82\x01\x90P\x91\x90PV[_`\x80\x83\x01_\x83\x01Q\x84\x82\x03_\x86\x01Ra)?\x82\x82a lV[\x91PP` \x83\x01Qa)T` \x86\x01\x82a\x1F\xEDV[P`@\x83\x01Qa)g`@\x86\x01\x82a\x1F\xEDV[P``\x83\x01Qa)z``\x86\x01\x82a\x1F\xEDV[P\x80\x91PP\x92\x91PPV[_a)\x90\x83\x83a)%V[\x90P\x92\x91PPV[_` \x82\x01\x90P\x91\x90PV[_a)\xAE\x82a(\xFCV[a)\xB8\x81\x85a)\x06V[\x93P\x83` \x82\x02\x85\x01a)\xCA\x85a)\x16V[\x80_[\x85\x81\x10\x15a*\x05W\x84\x84\x03\x89R\x81Qa)\xE6\x85\x82a)\x85V[\x94Pa)\xF1\x83a)\x98V[\x92P` \x8A\x01\x99PP`\x01\x81\x01\x90Pa)\xCDV[P\x82\x97P\x87\x95PPPPPP\x92\x91PPV[_` \x82\x01\x90P\x81\x81\x03_\x83\x01Ra*/\x81\x84a)\xA4V[\x90P\x92\x91PPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`\x11`\x04R`$_\xFD[_a*n\x82a\x1F-V[\x91Pa*y\x83a\x1F-V[\x92P\x82\x82\x02a*\x87\x81a\x1F-V[\x91P\x82\x82\x04\x84\x14\x83\x15\x17a*\x9EWa*\x9Da*7V[[P\x92\x91PPV[_a*\xAF\x82a\x1F-V[\x91Pa*\xBA\x83a\x1F-V[\x92P\x82\x82\x01\x90P\x80\x82\x11\x15a*\xD2Wa*\xD1a*7V[[\x92\x91PPV[_a*\xE2\x82a\x1F-V[\x91Pa*\xED\x83a\x1F-V[\x92P\x82\x82\x03\x90P\x81\x81\x11\x15a+\x05Wa+\x04a*7V[[\x92\x91PPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`\"`\x04R`$_\xFD[_`\x02\x82\x04\x90P`\x01\x82\x16\x80a+OW`\x7F\x82\x16\x91P[` \x82\x10\x81\x03a+bWa+aa+\x0BV[[P\x91\x90PV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`2`\x04R`$_\xFD[_\x81\x90P\x81_R` _ \x90P\x91\x90PV[_` `\x1F\x83\x01\x04\x90P\x91\x90PV[_\x82\x82\x1B\x90P\x92\x91PPV[_`\x08\x83\x02a+\xF1\x7F\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82a+\xB6V[a+\xFB\x86\x83a+\xB6V[\x95P\x80\x19\x84\x16\x93P\x80\x86\x16\x84\x17\x92PPP\x93\x92PPPV[_\x81\x90P\x91\x90PV[_a,6a,1a,,\x84a\x1F-V[a,\x13V[a\x1F-V[\x90P\x91\x90PV[_\x81\x90P\x91\x90PV[a,O\x83a,\x1CV[a,ca,[\x82a,=V[\x84\x84Ta+\xC2V[\x82UPPPPV[__\x90P\x90V[a,za,kV[a,\x85\x81\x84\x84a,FV[PPPV[[\x81\x81\x10\x15a,\xA8Wa,\x9D_\x82a,rV[`\x01\x81\x01\x90Pa,\x8BV[PPV[`\x1F\x82\x11\x15a,\xEDWa,\xBE\x81a+\x95V[a,\xC7\x84a+\xA7V[\x81\x01` \x85\x10\x15a,\xD6W\x81\x90P[a,\xEAa,\xE2\x85a+\xA7V[\x83\x01\x82a,\x8AV[PP[PPPV[_\x82\x82\x1C\x90P\x92\x91PPV[_a-\r_\x19\x84`\x08\x02a,\xF2V[\x19\x80\x83\x16\x91PP\x92\x91PPV[_a-%\x83\x83a,\xFEV[\x91P\x82`\x02\x02\x82\x17\x90P\x92\x91PPV[a->\x82a\x1F\xFCV[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a-WWa-Va!zV[[a-a\x82Ta+8V[a-l\x82\x82\x85a,\xACV[_` \x90P`\x1F\x83\x11`\x01\x81\x14a-\x9DW_\x84\x15a-\x8BW\x82\x87\x01Q\x90P[a-\x95\x85\x82a-\x1AV[\x86UPa-\xFCV[`\x1F\x19\x84\x16a-\xAB\x86a+\x95V[_[\x82\x81\x10\x15a-\xD2W\x84\x89\x01Q\x82U`\x01\x82\x01\x91P` \x85\x01\x94P` \x81\x01\x90Pa-\xADV[\x86\x83\x10\x15a-\xEFW\x84\x89\x01Qa-\xEB`\x1F\x89\x16\x82a,\xFEV[\x83UP[`\x01`\x02\x88\x02\x01\x88UPPP[PPPPPPV[_a.\x0E\x82a\x1F-V[\x91P\x7F\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x03a.@Wa.?a*7V[[`\x01\x82\x01\x90P\x91\x90PV[_a.U\x82a\x1F-V[\x91P_\x82\x03a.gWa.fa*7V[[`\x01\x82\x03\x90P\x91\x90PV[_`@\x82\x01\x90Pa.\x85_\x83\x01\x85a#MV[a.\x92` \x83\x01\x84a#MV[\x93\x92PPPV[_``\x82\x01\x90Pa.\xAC_\x83\x01\x86a#MV[a.\xB9` \x83\x01\x85a#MV[a.\xC6`@\x83\x01\x84a#MV[\x94\x93PPPPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`1`\x04R`$_\xFD\xFE\xA2dipfsX\"\x12 \xC0^\xF7y;z\0!'R\xB6\x14\xF8\xEBX\xFB@\x19\x19\xC5\xD2\xFF\x91\xBD\x18Bz l\xD4\xB0\xC3dsolcC\0\x08\x1C\x003";
    /// The bytecode of the contract.
    pub static ACCOUNTCONFIG_BYTECODE: ::ethers::core::types::Bytes = ::ethers::core::types::Bytes::from_static(
        __BYTECODE,
    );
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R4\x80\x15a\0\x0FW__\xFD[P`\x046\x10a\x015W_5`\xE0\x1C\x80cy\xB8\xE6\x91\x11a\0\xB6W\x80c\xBA\xC7\x10\xEA\x11a\0zW\x80c\xBA\xC7\x10\xEA\x14a\x03EW\x80c\xBD\x9A\xEDQ\x14a\x03aW\x80c\xC5\xF5\xB9\x84\x14a\x03}W\x80c\xC7\x04f\x8C\x14a\x03\x99W\x80c\xDB\xB1z\x0B\x14a\x03\xC9W\x80c\xF7\\\x8B-\x14a\x03\xE5Wa\x015V[\x80cy\xB8\xE6\x91\x14a\x02\x8FW\x80cz\xF3a\xEF\x14a\x02\xBFW\x80c\x8D\xA5\xCB[\x14a\x02\xEFW\x80c\x96\xA7\xCCT\x14a\x03\rW\x80c\xA6\xB6\xB6r\x14a\x03)Wa\x015V[\x80c]\x86\x1Cr\x11a\0\xFDW\x80c]\x86\x1Cr\x14a\x01\xEFW\x80cj=w\xA9\x14a\x02\x0BW\x80cn\x06\xAC\x9C\x14a\x02'W\x80cq\x9F\xACC\x14a\x02CW\x80ct\x9EM\x07\x14a\x02sWa\x015V[\x80c)\x1F\xF1\xEA\x14a\x019W\x80c/\xA9.A\x14a\x01iW\x80cG\x05\x16\x1E\x14a\x01\x85W\x80cIqy5\x14a\x01\xA3W\x80cT)p\xED\x14a\x01\xBFW[__\xFD[a\x01S`\x04\x806\x03\x81\x01\x90a\x01N\x91\x90a\x1F`V[a\x04\x15V[`@Qa\x01`\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[a\x01\x83`\x04\x806\x03\x81\x01\x90a\x01~\x91\x90a\"\x9EV[a\x06\xC6V[\0[a\x01\x8Da\x07\x8FV[`@Qa\x01\x9A\x91\x90a#\\V[`@Q\x80\x91\x03\x90\xF3[a\x01\xBD`\x04\x806\x03\x81\x01\x90a\x01\xB8\x91\x90a\x1F`V[a\x07\x95V[\0[a\x01\xD9`\x04\x806\x03\x81\x01\x90a\x01\xD4\x91\x90a\x1F`V[a\x08\x0BV[`@Qa\x01\xE6\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[a\x02\t`\x04\x806\x03\x81\x01\x90a\x02\x04\x91\x90a#uV[a\n\xBCV[\0[a\x02%`\x04\x806\x03\x81\x01\x90a\x02 \x91\x90a#\xC5V[a\x0B/V[\0[a\x02A`\x04\x806\x03\x81\x01\x90a\x02<\x91\x90a#uV[a\x0B\x9FV[\0[a\x02]`\x04\x806\x03\x81\x01\x90a\x02X\x91\x90a$aV[a\x0B\xECV[`@Qa\x02j\x91\x90a$\xA6V[`@Q\x80\x91\x03\x90\xF3[a\x02\x8D`\x04\x806\x03\x81\x01\x90a\x02\x88\x91\x90a%\xADV[a\x0C\xEFV[\0[a\x02\xA9`\x04\x806\x03\x81\x01\x90a\x02\xA4\x91\x90a&\xBAV[a\x0EkV[`@Qa\x02\xB6\x91\x90a#\\V[`@Q\x80\x91\x03\x90\xF3[a\x02\xD9`\x04\x806\x03\x81\x01\x90a\x02\xD4\x91\x90a#uV[a\x0E\xA5V[`@Qa\x02\xE6\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[a\x02\xF7a\x11\x15V[`@Qa\x03\x04\x91\x90a'7V[`@Q\x80\x91\x03\x90\xF3[a\x03'`\x04\x806\x03\x81\x01\x90a\x03\"\x91\x90a'PV[a\x11:V[\0[a\x03C`\x04\x806\x03\x81\x01\x90a\x03>\x91\x90a\"\x9EV[a\x12\x07V[\0[a\x03_`\x04\x806\x03\x81\x01\x90a\x03Z\x91\x90a#uV[a\x12uV[\0[a\x03{`\x04\x806\x03\x81\x01\x90a\x03v\x91\x90a(;V[a\x12\xC1V[\0[a\x03\x97`\x04\x806\x03\x81\x01\x90a\x03\x92\x91\x90a&\xBAV[a\x13\x9FV[\0[a\x03\xB3`\x04\x806\x03\x81\x01\x90a\x03\xAE\x91\x90a#uV[a\x13\xD8V[`@Qa\x03\xC0\x91\x90a*\x17V[`@Q\x80\x91\x03\x90\xF3[a\x03\xE3`\x04\x806\x03\x81\x01\x90a\x03\xDE\x91\x90a\"\x9EV[a\x16\x80V[\0[a\x03\xFF`\x04\x806\x03\x81\x01\x90a\x03\xFA\x91\x90a#uV[a\x17fV[`@Qa\x04\x0C\x91\x90a!RV[`@Q\x80\x91\x03\x90\xF3[``a\x04!\x85\x85a\x1A\0V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P_\x81`\x0F\x01_\x87\x81R` \x01\x90\x81R` \x01_ \x90Pa\x04V\x81`\x05\x01a\x1AQV[\x84\x11\x15a\x04oWa\x04i\x81`\x05\x01a\x1AQV[\x93P_\x94P[_\x84\x86a\x04|\x91\x90a*dV[\x90P_\x85\x82a\x04\x8B\x91\x90a*\xA5V[\x90Pa\x04\x99\x83`\x05\x01a\x1AQV[\x81\x11\x15a\x04\xAFWa\x04\xAC\x83`\x05\x01a\x1AQV[\x90P[_\x82\x82a\x04\xBC\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x04\xD9Wa\x04\xD8a!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x05\x12W\x81` \x01[a\x04\xFFa\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x04\xF7W\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\x06\xB4W\x86`\x15\x01_a\x05H\x83\x88a\x056\x91\x90a*\xA5V[\x89`\x05\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ `@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x05y\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x05\xA5\x90a+8V[\x80\x15a\x05\xF0W\x80`\x1F\x10a\x05\xC7Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x05\xF0V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x05\xD3W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x06\t\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x065\x90a+8V[\x80\x15a\x06\x80W\x80`\x1F\x10a\x06WWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x06\x80V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x06cW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\x06\x9CWa\x06\x9Ba+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x05\x1AV[P\x80\x96PPPPPPP\x94\x93PPPPV[a\x06\xD0\x85\x85a\x1A\0V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90Pa\x07\x0B\x84\x82`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x83\x81`\x12\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x01\x81\x90UP\x82\x81`\x12\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01\x90\x81a\x07I\x91\x90a-5V[P\x81\x81`\x12\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x02\x01\x90\x81a\x07m\x91\x90a-5V[P\x80`\x17\x01_\x81T\x80\x92\x91\x90a\x07\x82\x90a.\x04V[\x91\x90PUPPPPPPPV[`\x03T\x81V[a\x07\x9E\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ `\x0C\x01_\x85\x81R` \x01\x90\x81R` \x01_ \x90P\x83\x81`\x03\x01\x81\x90UP\x82\x81`\x04\x01\x81\x90UP\x81\x81`\x05\x01\x81\x90UPa\x08\x03\x84__\x88\x81R` \x01\x90\x81R` \x01_ `\n\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPPV[``a\x08\x17\x85\x85a\x1A\0V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P_\x81`\x0F\x01_\x87\x81R` \x01\x90\x81R` \x01_ \x90Pa\x08L\x81`\x03\x01a\x1AQV[\x84\x11\x15a\x08eWa\x08_\x81`\x03\x01a\x1AQV[\x93P_\x94P[_\x84\x86a\x08r\x91\x90a*dV[\x90P_\x85\x82a\x08\x81\x91\x90a*\xA5V[\x90Pa\x08\x8F\x83`\x03\x01a\x1AQV[\x81\x11\x15a\x08\xA5Wa\x08\xA2\x83`\x03\x01a\x1AQV[\x90P[_\x82\x82a\x08\xB2\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x08\xCFWa\x08\xCEa!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\t\x08W\x81` \x01[a\x08\xF5a\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x08\xEDW\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\n\xAAW\x86`\x12\x01_a\t>\x83\x88a\t,\x91\x90a*\xA5V[\x89`\x03\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ `@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\to\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\t\x9B\x90a+8V[\x80\x15a\t\xE6W\x80`\x1F\x10a\t\xBDWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\t\xE6V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\t\xC9W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\t\xFF\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\n+\x90a+8V[\x80\x15a\nvW\x80`\x1F\x10a\nMWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\nvV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\nYW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\n\x92Wa\n\x91a+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\t\x10V[P\x80\x96PPPPPPP\x94\x93PPPPV[a\n\xC7\x83\x83\x83a\x1A\xDFV[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x0B\x02\x82\x82`\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x1B4\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P_\x81`\x17\x01T\x11\x15a\x0B)W\x80`\x17\x01_\x81T\x80\x92\x91\x90a\x0B#\x90a.KV[\x91\x90PUP[PPPPV[a\x0B9\x84\x84a\x1BKV[___\x86\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81`\x0C\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x01`\x01\x01\x90\x81a\x0Bq\x91\x90a-5V[P\x81\x81`\x0C\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x01`\x02\x01\x90\x81a\x0B\x97\x91\x90a-5V[PPPPPPV[a\x0B\xAA\x83\x83\x83a\x1B\x9CV[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x0B\xE5\x82\x82`\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x05\x01a\x1B4\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[____\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81_\x01_\x01T\x14a\x0C\x15W_\x91PPa\x0C\xEAV[`\x02_\x90T\x90a\x01\0\n\x90\x04s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x163s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x14\x80\x15a\x0C\x85WP`\x01\x15\x15\x81`\x13\x01_\x90T\x90a\x01\0\n\x90\x04`\xFF\x16\x15\x15\x14[\x15a\x0C\x94W`\x01\x91PPa\x0C\xEAV[3s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x81`\t\x01_\x90T\x90a\x01\0\n\x90\x04s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x14\x91PP[\x91\x90PV[a\x0C\xF8\x87a\x1A\x92V[___\x89\x81R` \x01\x90\x81R` \x01_ \x90Pa\r%\x81`\x18\x01T\x82`\r\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P_\x81`\x0F\x01_\x83`\x18\x01T\x81R` \x01\x90\x81R` \x01_ \x90P\x81`\x18\x01T\x81_\x01_\x01\x81\x90UP\x87\x81_\x01`\x01\x01\x90\x81a\ra\x91\x90a-5V[P\x86\x81_\x01`\x02\x01\x90\x81a\ru\x91\x90a-5V[P__\x90P[\x86Q\x81\x10\x15a\r\xC2Wa\r\xB4\x87\x82\x81Q\x81\x10a\r\x9AWa\r\x99a+hV[[` \x02` \x01\x01Q\x83`\x03\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\r{V[P__\x90P[\x85Q\x81\x10\x15a\x0E\x0FWa\x0E\x01\x86\x82\x81Q\x81\x10a\r\xE7Wa\r\xE6a+hV[[` \x02` \x01\x01Q\x83`\x05\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\r\xC8V[P\x83\x81`\x07\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x82\x81`\x07\x01`\x01a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x81`\x18\x01_\x81T\x80\x92\x91\x90a\x0E[\x90a.\x04V[\x91\x90PUPPPPPPPPPPV[_a\x0Eu\x83a\x1A\x92V[___\x85\x81R` \x01\x90\x81R` \x01_ \x90P\x80`\x14\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x91PP\x92\x91PPV[``a\x0E\xB0\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ \x90P`\x03T\x83\x11\x15a\x0E\xD6W`\x03T\x92P_\x93P[_\x83\x85a\x0E\xE3\x91\x90a*dV[\x90P_\x84\x82a\x0E\xF2\x91\x90a*\xA5V[\x90P`\x03T\x81\x11\x15a\x0F\x04W`\x03T\x90P[_\x82\x82a\x0F\x11\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x0F.Wa\x0F-a!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x0FgW\x81` \x01[a\x0FTa\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x0FLW\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\x11\x05W\x85`\x15\x01_`\x01_\x84\x89a\x0F\x8B\x91\x90a*\xA5V[\x81R` \x01\x90\x81R` \x01_ T\x81R` \x01\x90\x81R` \x01_ `@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x0F\xCA\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x0F\xF6\x90a+8V[\x80\x15a\x10AW\x80`\x1F\x10a\x10\x18Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x10AV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x10$W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x10Z\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x10\x86\x90a+8V[\x80\x15a\x10\xD1W\x80`\x1F\x10a\x10\xA8Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x10\xD1V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x10\xB4W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\x10\xEDWa\x10\xECa+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x0FoV[P\x80\x95PPPPPP\x93\x92PPPV[`\x02_\x90T\x90a\x01\0\n\x90\x04s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x81V[a\x11D\x86\x86a\x1A\0V[___\x88\x81R` \x01\x90\x81R` \x01_ \x90P\x84\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ _\x01`\x01\x01\x90\x81a\x11|\x91\x90a-5V[P\x83\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ _\x01`\x02\x01\x90\x81a\x11\xA2\x91\x90a-5V[P\x82\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ `\x07\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x81\x81`\x0F\x01_\x88\x81R` \x01\x90\x81R` \x01_ `\x07\x01`\x01a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UPPPPPPPPV[a\x12\x12\x85\x84\x86a\x1A\xDFV[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81`\x12\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x01\x01\x90\x81a\x12H\x91\x90a-5V[P\x81\x81`\x12\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x02\x01\x90\x81a\x12l\x91\x90a-5V[PPPPPPPV[a\x12\x7F\x83\x83a\x1A\0V[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x12\xBA\x82\x82`\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x05\x01a\x1A{\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[___\x88\x81R` \x01\x90\x81R` \x01_ \x90P\x85\x81`\x13\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UP\x82\x81`\t\x01_a\x01\0\n\x81T\x81s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x02\x19\x16\x90\x83s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x16\x02\x17\x90UP\x86\x81_\x01_\x01\x81\x90UP\x84\x81_\x01`\x01\x01\x90\x81a\x13N\x91\x90a-5V[P\x83\x81_\x01`\x02\x01\x90\x81a\x13b\x91\x90a-5V[P\x86\x81`\x03\x01`\x03\x01\x81\x90UPc\x12\xCC\x03\0Ba\x13\x7F\x91\x90a*\xA5V[\x81`\x03\x01`\x04\x01\x81\x90UP\x81\x81`\x03\x01`\x05\x01\x81\x90UPPPPPPPPV[a\x13\xA9\x82\x82a\x1BKV[___\x84\x81R` \x01\x90\x81R` \x01_ \x90Pa\x13\xD2\x82\x82`\n\x01a\x1B4\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPV[``a\x13\xE3\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ \x90P_a\x14\x03\x82`\n\x01a\x1AQV[\x90P\x80\x84\x11\x15a\x14\x14W\x80\x93P_\x94P[_\x84\x86a\x14!\x91\x90a*dV[\x90P_\x85\x82a\x140\x91\x90a*\xA5V[\x90P\x82\x81\x11\x15a\x14>W\x82\x90P[_\x82\x82a\x14K\x91\x90a*\xD8V[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x14dWa\x14ca!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x14\x9DW\x81` \x01[a\x14\x8Aa\x1E\xF1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x14\x82W\x90P[P\x90P__\x90P[\x84\x81\x10\x15a\x16pW\x85`\x0C\x01_a\x14\xD3\x83\x87a\x14\xC1\x91\x90a*\xA5V[\x89`\n\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ `@Q\x80`\x80\x01`@R\x90\x81_\x82\x01`@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x15\x13\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x15?\x90a+8V[\x80\x15a\x15\x8AW\x80`\x1F\x10a\x15aWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x15\x8AV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x15mW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x15\xA3\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x15\xCF\x90a+8V[\x80\x15a\x16\x1AW\x80`\x1F\x10a\x15\xF1Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x16\x1AV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x15\xFDW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x81R` \x01`\x03\x82\x01T\x81R` \x01`\x04\x82\x01T\x81R` \x01`\x05\x82\x01T\x81RPP\x82\x82\x81Q\x81\x10a\x16XWa\x16Wa+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x14\xA5V[P\x80\x95PPPPPP\x93\x92PPPV[a\x16\x89\x85a\x1A\x92V[___\x87\x81R` \x01\x90\x81R` \x01_ \x90P\x83\x81`\x14\x01_\x87\x81R` \x01\x90\x81R` \x01_ \x81\x90UP\x84\x81`\x15\x01_\x87\x81R` \x01\x90\x81R` \x01_ _\x01\x81\x90UP\x82\x81`\x15\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x01\x01\x90\x81a\x16\xF1\x91\x90a-5V[P\x81\x81`\x15\x01_\x87\x81R` \x01\x90\x81R` \x01_ `\x02\x01\x90\x81a\x17\x15\x91\x90a-5V[P\x84`\x01_`\x03T\x81R` \x01\x90\x81R` \x01_ \x81\x90UP\x80`\x16\x01_\x81T\x80\x92\x91\x90a\x17B\x90a.\x04V[\x91\x90PUP`\x03_\x81T\x80\x92\x91\x90a\x17Y\x90a.\x04V[\x91\x90PUPPPPPPPV[``a\x17q\x84a\x1A\x92V[___\x86\x81R` \x01\x90\x81R` \x01_ \x90Pa\x17\x90\x81`\r\x01a\x1AQV[\x83\x11\x15a\x17\xA9Wa\x17\xA3\x81`\r\x01a\x1AQV[\x92P_\x93P[_\x83\x85a\x17\xB6\x91\x90a*dV[\x90P_\x84\x82a\x17\xC5\x91\x90a*\xA5V[\x90Pa\x17\xD3\x83`\r\x01a\x1AQV[\x81\x11\x15a\x17\xE9Wa\x17\xE6\x83`\r\x01a\x1AQV[\x90P[_\x82\x82a\x17\xF6\x91\x90a*\xD8V[\x90P_\x81g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x18\x13Wa\x18\x12a!zV[[`@Q\x90\x80\x82R\x80` \x02` \x01\x82\x01`@R\x80\x15a\x18LW\x81` \x01[a\x189a\x1E\xD1V[\x81R` \x01\x90`\x01\x90\x03\x90\x81a\x181W\x90P[P\x90P__\x90P[\x82\x81\x10\x15a\x19\xF0W\x85`\x0F\x01_a\x18\x82\x83\x88a\x18p\x91\x90a*\xA5V[\x89`\r\x01a\x1Ad\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x81R` \x01\x90\x81R` \x01_ _\x01`@Q\x80``\x01`@R\x90\x81_\x82\x01T\x81R` \x01`\x01\x82\x01\x80Ta\x18\xB5\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x18\xE1\x90a+8V[\x80\x15a\x19,W\x80`\x1F\x10a\x19\x03Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x19,V[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x19\x0FW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81R` \x01`\x02\x82\x01\x80Ta\x19E\x90a+8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x19q\x90a+8V[\x80\x15a\x19\xBCW\x80`\x1F\x10a\x19\x93Wa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x19\xBCV[\x82\x01\x91\x90_R` _ \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x19\x9FW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81RPP\x82\x82\x81Q\x81\x10a\x19\xD8Wa\x19\xD7a+hV[[` \x02` \x01\x01\x81\x90RP\x80\x80`\x01\x01\x91PPa\x18TV[P\x80\x95PPPPPP\x93\x92PPPV[a\x1A\n\x82\x82a\x1B\xF1V[a\x1AMW\x81\x81`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1AD\x92\x91\x90a.rV[`@Q\x80\x91\x03\x90\xFD[PPV[_a\x1A]\x82_\x01a\x1C1V[\x90P\x91\x90PV[_a\x1Aq\x83_\x01\x83a\x1C@V[_\x1C\x90P\x92\x91PPV[_a\x1A\x8A\x83_\x01\x83_\x1Ba\x1CgV[\x90P\x92\x91PPV[a\x1A\x9B\x81a\x0B\xECV[a\x1A\xDCW\x80`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1A\xD3\x91\x90a#\\V[`@Q\x80\x91\x03\x90\xFD[PV[a\x1A\xEA\x83\x83\x83a\x1C\xCEV[a\x1B/W\x82\x82\x82`@Q\x7F\xEF%\xD0-\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1B&\x93\x92\x91\x90a.\x99V[`@Q\x80\x91\x03\x90\xFD[PPPV[_a\x1BC\x83_\x01\x83_\x1Ba\x1D\x19V[\x90P\x92\x91PPV[a\x1BU\x82\x82a\x1E\x15V[a\x1B\x98W\x81\x81`@Q\x7Ft\x8Eq*\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1B\x8F\x92\x91\x90a.rV[`@Q\x80\x91\x03\x90\xFD[PPV[a\x1B\xA7\x83\x83\x83a\x1EOV[a\x1B\xECW\x82\x82\x82`@Q\x7Fy\x16xX\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x1B\xE3\x93\x92\x91\x90a.\x99V[`@Q\x80\x91\x03\x90\xFD[PPPV[_a\x1B\xFB\x83a\x1A\x92V[___\x85\x81R` \x01\x90\x81R` \x01_ `\x0F\x01_\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81_\x01_\x01T\x14\x91PP\x92\x91PPV[_\x81_\x01\x80T\x90P\x90P\x91\x90PV[_\x82_\x01\x82\x81T\x81\x10a\x1CVWa\x1CUa+hV[[\x90_R` _ \x01T\x90P\x92\x91PPV[_a\x1Cr\x83\x83a\x1E\x9AV[a\x1C\xC4W\x82_\x01\x82\x90\x80`\x01\x81T\x01\x80\x82U\x80\x91PP`\x01\x90\x03\x90_R` _ \x01_\x90\x91\x90\x91\x90\x91PU\x82_\x01\x80T\x90P\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ \x81\x90UP`\x01\x90Pa\x1C\xC8V[_\x90P[\x92\x91PPV[_a\x1C\xD9\x84\x84a\x1A\0V[a\x1D\x10\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x1E\xBA\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x90P_\x81\x14a\x1E\nW_`\x01\x82a\x1DF\x91\x90a*\xD8V[\x90P_`\x01\x86_\x01\x80T\x90Pa\x1D\\\x91\x90a*\xD8V[\x90P\x80\x82\x14a\x1D\xC2W_\x86_\x01\x82\x81T\x81\x10a\x1D{Wa\x1Dza+hV[[\x90_R` _ \x01T\x90P\x80\x87_\x01\x84\x81T\x81\x10a\x1D\x9CWa\x1D\x9Ba+hV[[\x90_R` _ \x01\x81\x90UP\x83\x87`\x01\x01_\x83\x81R` \x01\x90\x81R` \x01_ \x81\x90UPP[\x85_\x01\x80T\x80a\x1D\xD5Wa\x1D\xD4a.\xCEV[[`\x01\x90\x03\x81\x81\x90_R` _ \x01_\x90U\x90U\x85`\x01\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x90U`\x01\x93PPPPa\x1E\x0FV[_\x91PP[\x92\x91PPV[_a\x1E\x1F\x83a\x1A\x92V[\x81__\x85\x81R` \x01\x90\x81R` \x01_ `\x0C\x01_\x84\x81R` \x01\x90\x81R` \x01_ `\x03\x01T\x14\x90P\x92\x91PPV[_a\x1EZ\x84\x84a\x1A\0V[a\x1E\x91\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x0F\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x05\x01a\x1E\xBA\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x14\x15\x90P\x92\x91PPV[_a\x1E\xC9\x83_\x01\x83_\x1Ba\x1E\x9AV[\x90P\x92\x91PPV[`@Q\x80``\x01`@R\x80_\x81R` \x01``\x81R` \x01``\x81RP\x90V[`@Q\x80`\x80\x01`@R\x80a\x1F\x04a\x1E\xD1V[\x81R` \x01_\x81R` \x01_\x81R` \x01_\x81RP\x90V[_`@Q\x90P\x90V[__\xFD[__\xFD[_\x81\x90P\x91\x90PV[a\x1F?\x81a\x1F-V[\x81\x14a\x1FIW__\xFD[PV[_\x815\x90Pa\x1FZ\x81a\x1F6V[\x92\x91PPV[____`\x80\x85\x87\x03\x12\x15a\x1FxWa\x1Fwa\x1F%V[[_a\x1F\x85\x87\x82\x88\x01a\x1FLV[\x94PP` a\x1F\x96\x87\x82\x88\x01a\x1FLV[\x93PP`@a\x1F\xA7\x87\x82\x88\x01a\x1FLV[\x92PP``a\x1F\xB8\x87\x82\x88\x01a\x1FLV[\x91PP\x92\x95\x91\x94P\x92PV[_\x81Q\x90P\x91\x90PV[_\x82\x82R` \x82\x01\x90P\x92\x91PPV[_\x81\x90P` \x82\x01\x90P\x91\x90PV[a\x1F\xF6\x81a\x1F-V[\x82RPPV[_\x81Q\x90P\x91\x90PV[_\x82\x82R` \x82\x01\x90P\x92\x91PPV[\x82\x81\x83^_\x83\x83\x01RPPPV[_`\x1F\x19`\x1F\x83\x01\x16\x90P\x91\x90PV[_a >\x82a\x1F\xFCV[a H\x81\x85a \x06V[\x93Pa X\x81\x85` \x86\x01a \x16V[a a\x81a $V[\x84\x01\x91PP\x92\x91PPV[_``\x83\x01_\x83\x01Qa \x81_\x86\x01\x82a\x1F\xEDV[P` \x83\x01Q\x84\x82\x03` \x86\x01Ra \x99\x82\x82a 4V[\x91PP`@\x83\x01Q\x84\x82\x03`@\x86\x01Ra \xB3\x82\x82a 4V[\x91PP\x80\x91PP\x92\x91PPV[_a \xCB\x83\x83a lV[\x90P\x92\x91PPV[_` \x82\x01\x90P\x91\x90PV[_a \xE9\x82a\x1F\xC4V[a \xF3\x81\x85a\x1F\xCEV[\x93P\x83` \x82\x02\x85\x01a!\x05\x85a\x1F\xDEV[\x80_[\x85\x81\x10\x15a!@W\x84\x84\x03\x89R\x81Qa!!\x85\x82a \xC0V[\x94Pa!,\x83a \xD3V[\x92P` \x8A\x01\x99PP`\x01\x81\x01\x90Pa!\x08V[P\x82\x97P\x87\x95PPPPPP\x92\x91PPV[_` \x82\x01\x90P\x81\x81\x03_\x83\x01Ra!j\x81\x84a \xDFV[\x90P\x92\x91PPV[__\xFD[__\xFD[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`A`\x04R`$_\xFD[a!\xB0\x82a $V[\x81\x01\x81\x81\x10g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x17\x15a!\xCFWa!\xCEa!zV[[\x80`@RPPPV[_a!\xE1a\x1F\x1CV[\x90Pa!\xED\x82\x82a!\xA7V[\x91\x90PV[_g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x15a\"\x0CWa\"\x0Ba!zV[[a\"\x15\x82a $V[\x90P` \x81\x01\x90P\x91\x90PV[\x82\x81\x837_\x83\x83\x01RPPPV[_a\"Ba\"=\x84a!\xF2V[a!\xD8V[\x90P\x82\x81R` \x81\x01\x84\x84\x84\x01\x11\x15a\"^Wa\"]a!vV[[a\"i\x84\x82\x85a\"\"V[P\x93\x92PPPV[_\x82`\x1F\x83\x01\x12a\"\x85Wa\"\x84a!rV[[\x815a\"\x95\x84\x82` \x86\x01a\"0V[\x91PP\x92\x91PPV[_____`\xA0\x86\x88\x03\x12\x15a\"\xB7Wa\"\xB6a\x1F%V[[_a\"\xC4\x88\x82\x89\x01a\x1FLV[\x95PP` a\"\xD5\x88\x82\x89\x01a\x1FLV[\x94PP`@a\"\xE6\x88\x82\x89\x01a\x1FLV[\x93PP``\x86\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a#\x07Wa#\x06a\x1F)V[[a#\x13\x88\x82\x89\x01a\"qV[\x92PP`\x80\x86\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a#4Wa#3a\x1F)V[[a#@\x88\x82\x89\x01a\"qV[\x91PP\x92\x95P\x92\x95\x90\x93PV[a#V\x81a\x1F-V[\x82RPPV[_` \x82\x01\x90Pa#o_\x83\x01\x84a#MV[\x92\x91PPV[___``\x84\x86\x03\x12\x15a#\x8CWa#\x8Ba\x1F%V[[_a#\x99\x86\x82\x87\x01a\x1FLV[\x93PP` a#\xAA\x86\x82\x87\x01a\x1FLV[\x92PP`@a#\xBB\x86\x82\x87\x01a\x1FLV[\x91PP\x92P\x92P\x92V[____`\x80\x85\x87\x03\x12\x15a#\xDDWa#\xDCa\x1F%V[[_a#\xEA\x87\x82\x88\x01a\x1FLV[\x94PP` a#\xFB\x87\x82\x88\x01a\x1FLV[\x93PP`@\x85\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a$\x1CWa$\x1Ba\x1F)V[[a$(\x87\x82\x88\x01a\"qV[\x92PP``\x85\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a$IWa$Ha\x1F)V[[a$U\x87\x82\x88\x01a\"qV[\x91PP\x92\x95\x91\x94P\x92PV[_` \x82\x84\x03\x12\x15a$vWa$ua\x1F%V[[_a$\x83\x84\x82\x85\x01a\x1FLV[\x91PP\x92\x91PPV[_\x81\x15\x15\x90P\x91\x90PV[a$\xA0\x81a$\x8CV[\x82RPPV[_` \x82\x01\x90Pa$\xB9_\x83\x01\x84a$\x97V[\x92\x91PPV[_g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x15a$\xD9Wa$\xD8a!zV[[` \x82\x02\x90P` \x81\x01\x90P\x91\x90PV[__\xFD[_a%\0a$\xFB\x84a$\xBFV[a!\xD8V[\x90P\x80\x83\x82R` \x82\x01\x90P` \x84\x02\x83\x01\x85\x81\x11\x15a%#Wa%\"a$\xEAV[[\x83[\x81\x81\x10\x15a%LW\x80a%8\x88\x82a\x1FLV[\x84R` \x84\x01\x93PP` \x81\x01\x90Pa%%V[PPP\x93\x92PPPV[_\x82`\x1F\x83\x01\x12a%jWa%ia!rV[[\x815a%z\x84\x82` \x86\x01a$\xEEV[\x91PP\x92\x91PPV[a%\x8C\x81a$\x8CV[\x81\x14a%\x96W__\xFD[PV[_\x815\x90Pa%\xA7\x81a%\x83V[\x92\x91PPV[_______`\xE0\x88\x8A\x03\x12\x15a%\xC8Wa%\xC7a\x1F%V[[_a%\xD5\x8A\x82\x8B\x01a\x1FLV[\x97PP` \x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a%\xF6Wa%\xF5a\x1F)V[[a&\x02\x8A\x82\x8B\x01a\"qV[\x96PP`@\x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a&#Wa&\"a\x1F)V[[a&/\x8A\x82\x8B\x01a\"qV[\x95PP``\x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a&PWa&Oa\x1F)V[[a&\\\x8A\x82\x8B\x01a%VV[\x94PP`\x80\x88\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a&}Wa&|a\x1F)V[[a&\x89\x8A\x82\x8B\x01a%VV[\x93PP`\xA0a&\x9A\x8A\x82\x8B\x01a%\x99V[\x92PP`\xC0a&\xAB\x8A\x82\x8B\x01a%\x99V[\x91PP\x92\x95\x98\x91\x94\x97P\x92\x95PV[__`@\x83\x85\x03\x12\x15a&\xD0Wa&\xCFa\x1F%V[[_a&\xDD\x85\x82\x86\x01a\x1FLV[\x92PP` a&\xEE\x85\x82\x86\x01a\x1FLV[\x91PP\x92P\x92\x90PV[_s\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x16\x90P\x91\x90PV[_a'!\x82a&\xF8V[\x90P\x91\x90PV[a'1\x81a'\x17V[\x82RPPV[_` \x82\x01\x90Pa'J_\x83\x01\x84a'(V[\x92\x91PPV[______`\xC0\x87\x89\x03\x12\x15a'jWa'ia\x1F%V[[_a'w\x89\x82\x8A\x01a\x1FLV[\x96PP` a'\x88\x89\x82\x8A\x01a\x1FLV[\x95PP`@\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a'\xA9Wa'\xA8a\x1F)V[[a'\xB5\x89\x82\x8A\x01a\"qV[\x94PP``\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a'\xD6Wa'\xD5a\x1F)V[[a'\xE2\x89\x82\x8A\x01a\"qV[\x93PP`\x80a'\xF3\x89\x82\x8A\x01a%\x99V[\x92PP`\xA0a(\x04\x89\x82\x8A\x01a%\x99V[\x91PP\x92\x95P\x92\x95P\x92\x95V[a(\x1A\x81a'\x17V[\x81\x14a($W__\xFD[PV[_\x815\x90Pa(5\x81a(\x11V[\x92\x91PPV[______`\xC0\x87\x89\x03\x12\x15a(UWa(Ta\x1F%V[[_a(b\x89\x82\x8A\x01a\x1FLV[\x96PP` a(s\x89\x82\x8A\x01a%\x99V[\x95PP`@\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a(\x94Wa(\x93a\x1F)V[[a(\xA0\x89\x82\x8A\x01a\"qV[\x94PP``\x87\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a(\xC1Wa(\xC0a\x1F)V[[a(\xCD\x89\x82\x8A\x01a\"qV[\x93PP`\x80a(\xDE\x89\x82\x8A\x01a('V[\x92PP`\xA0a(\xEF\x89\x82\x8A\x01a\x1FLV[\x91PP\x92\x95P\x92\x95P\x92\x95V[_\x81Q\x90P\x91\x90PV[_\x82\x82R` \x82\x01\x90P\x92\x91PPV[_\x81\x90P` \x82\x01\x90P\x91\x90PV[_`\x80\x83\x01_\x83\x01Q\x84\x82\x03_\x86\x01Ra)?\x82\x82a lV[\x91PP` \x83\x01Qa)T` \x86\x01\x82a\x1F\xEDV[P`@\x83\x01Qa)g`@\x86\x01\x82a\x1F\xEDV[P``\x83\x01Qa)z``\x86\x01\x82a\x1F\xEDV[P\x80\x91PP\x92\x91PPV[_a)\x90\x83\x83a)%V[\x90P\x92\x91PPV[_` \x82\x01\x90P\x91\x90PV[_a)\xAE\x82a(\xFCV[a)\xB8\x81\x85a)\x06V[\x93P\x83` \x82\x02\x85\x01a)\xCA\x85a)\x16V[\x80_[\x85\x81\x10\x15a*\x05W\x84\x84\x03\x89R\x81Qa)\xE6\x85\x82a)\x85V[\x94Pa)\xF1\x83a)\x98V[\x92P` \x8A\x01\x99PP`\x01\x81\x01\x90Pa)\xCDV[P\x82\x97P\x87\x95PPPPPP\x92\x91PPV[_` \x82\x01\x90P\x81\x81\x03_\x83\x01Ra*/\x81\x84a)\xA4V[\x90P\x92\x91PPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`\x11`\x04R`$_\xFD[_a*n\x82a\x1F-V[\x91Pa*y\x83a\x1F-V[\x92P\x82\x82\x02a*\x87\x81a\x1F-V[\x91P\x82\x82\x04\x84\x14\x83\x15\x17a*\x9EWa*\x9Da*7V[[P\x92\x91PPV[_a*\xAF\x82a\x1F-V[\x91Pa*\xBA\x83a\x1F-V[\x92P\x82\x82\x01\x90P\x80\x82\x11\x15a*\xD2Wa*\xD1a*7V[[\x92\x91PPV[_a*\xE2\x82a\x1F-V[\x91Pa*\xED\x83a\x1F-V[\x92P\x82\x82\x03\x90P\x81\x81\x11\x15a+\x05Wa+\x04a*7V[[\x92\x91PPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`\"`\x04R`$_\xFD[_`\x02\x82\x04\x90P`\x01\x82\x16\x80a+OW`\x7F\x82\x16\x91P[` \x82\x10\x81\x03a+bWa+aa+\x0BV[[P\x91\x90PV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`2`\x04R`$_\xFD[_\x81\x90P\x81_R` _ \x90P\x91\x90PV[_` `\x1F\x83\x01\x04\x90P\x91\x90PV[_\x82\x82\x1B\x90P\x92\x91PPV[_`\x08\x83\x02a+\xF1\x7F\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82a+\xB6V[a+\xFB\x86\x83a+\xB6V[\x95P\x80\x19\x84\x16\x93P\x80\x86\x16\x84\x17\x92PPP\x93\x92PPPV[_\x81\x90P\x91\x90PV[_a,6a,1a,,\x84a\x1F-V[a,\x13V[a\x1F-V[\x90P\x91\x90PV[_\x81\x90P\x91\x90PV[a,O\x83a,\x1CV[a,ca,[\x82a,=V[\x84\x84Ta+\xC2V[\x82UPPPPV[__\x90P\x90V[a,za,kV[a,\x85\x81\x84\x84a,FV[PPPV[[\x81\x81\x10\x15a,\xA8Wa,\x9D_\x82a,rV[`\x01\x81\x01\x90Pa,\x8BV[PPV[`\x1F\x82\x11\x15a,\xEDWa,\xBE\x81a+\x95V[a,\xC7\x84a+\xA7V[\x81\x01` \x85\x10\x15a,\xD6W\x81\x90P[a,\xEAa,\xE2\x85a+\xA7V[\x83\x01\x82a,\x8AV[PP[PPPV[_\x82\x82\x1C\x90P\x92\x91PPV[_a-\r_\x19\x84`\x08\x02a,\xF2V[\x19\x80\x83\x16\x91PP\x92\x91PPV[_a-%\x83\x83a,\xFEV[\x91P\x82`\x02\x02\x82\x17\x90P\x92\x91PPV[a->\x82a\x1F\xFCV[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a-WWa-Va!zV[[a-a\x82Ta+8V[a-l\x82\x82\x85a,\xACV[_` \x90P`\x1F\x83\x11`\x01\x81\x14a-\x9DW_\x84\x15a-\x8BW\x82\x87\x01Q\x90P[a-\x95\x85\x82a-\x1AV[\x86UPa-\xFCV[`\x1F\x19\x84\x16a-\xAB\x86a+\x95V[_[\x82\x81\x10\x15a-\xD2W\x84\x89\x01Q\x82U`\x01\x82\x01\x91P` \x85\x01\x94P` \x81\x01\x90Pa-\xADV[\x86\x83\x10\x15a-\xEFW\x84\x89\x01Qa-\xEB`\x1F\x89\x16\x82a,\xFEV[\x83UP[`\x01`\x02\x88\x02\x01\x88UPPP[PPPPPPV[_a.\x0E\x82a\x1F-V[\x91P\x7F\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x03a.@Wa.?a*7V[[`\x01\x82\x01\x90P\x91\x90PV[_a.U\x82a\x1F-V[\x91P_\x82\x03a.gWa.fa*7V[[`\x01\x82\x03\x90P\x91\x90PV[_`@\x82\x01\x90Pa.\x85_\x83\x01\x85a#MV[a.\x92` \x83\x01\x84a#MV[\x93\x92PPPV[_``\x82\x01\x90Pa.\xAC_\x83\x01\x86a#MV[a.\xB9` \x83\x01\x85a#MV[a.\xC6`@\x83\x01\x84a#MV[\x94\x93PPPPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`1`\x04R`$_\xFD\xFE\xA2dipfsX\"\x12 \xC0^\xF7y;z\0!'R\xB6\x14\xF8\xEBX\xFB@\x19\x19\xC5\xD2\xFF\x91\xBD\x18Bz l\xD4\xB0\xC3dsolcC\0\x08\x1C\x003";
    /// The deployed bytecode of the contract.
    pub static ACCOUNTCONFIG_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes = ::ethers::core::types::Bytes::from_static(
        __DEPLOYED_BYTECODE,
    );
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
            Self(
                ::ethers::contract::Contract::new(
                    address.into(),
                    ACCOUNTCONFIG_ABI.clone(),
                    client,
                ),
            )
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
                ACCOUNTCONFIG_ABI.clone(),
                ACCOUNTCONFIG_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
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
        ///Calls the contract's `addApiKey` (0x49717935) function
        pub fn add_api_key(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            usage_api_key_hash: ::ethers::core::types::U256,
            expiration: ::ethers::core::types::U256,
            balance: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [73, 113, 121, 53],
                    (account_api_key_hash, usage_api_key_hash, expiration, balance),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addGroup` (0x749e4d07) function
        pub fn add_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
            permitted_actions: ::std::vec::Vec<::ethers::core::types::U256>,
            wallets: ::std::vec::Vec<::ethers::core::types::U256>,
            all_wallets_permitted: bool,
            all_actions_permitted: bool,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [116, 158, 77, 7],
                    (
                        account_api_key_hash,
                        name,
                        description,
                        permitted_actions,
                        wallets,
                        all_wallets_permitted,
                        all_actions_permitted,
                    ),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addWalletToGroup` (0xbac710ea) function
        pub fn add_wallet_to_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            wallet_address_hash: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [186, 199, 16, 234],
                    (account_api_key_hash, group_id, wallet_address_hash),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `getWalletDerivation` (0x79b8e691) function
        pub fn get_wallet_derivation(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            wallet_address_hash: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash(
                    [121, 184, 230, 145],
                    (account_api_key_hash, wallet_address_hash),
                )
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
        ) -> ::ethers::contract::builders::ContractCall<
            M,
            ::std::vec::Vec<UsageApiKey>,
        > {
            self.0
                .method_hash(
                    [199, 4, 102, 140],
                    (account_api_key_hash, page_number, page_size),
                )
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
        ///Calls the contract's `listWallets` (0x7af361ef) function
        pub fn list_wallets(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            page_number: ::ethers::core::types::U256,
            page_size: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<Metadata>> {
            self.0
                .method_hash(
                    [122, 243, 97, 239],
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
        ) -> ::ethers::contract::builders::ContractCall<M, ::std::vec::Vec<Metadata>> {
            self.0
                .method_hash(
                    [41, 31, 241, 234],
                    (account_api_key_hash, group_id, page_number, page_size),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `newAccount` (0xbd9aed51) function
        pub fn new_account(
            &self,
            api_key_hash: ::ethers::core::types::U256,
            managed: bool,
            account_name: ::std::string::String,
            account_description: ::std::string::String,
            creator_wallet_address: ::ethers::core::types::Address,
            initial_balance: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [189, 154, 237, 81],
                    (
                        api_key_hash,
                        managed,
                        account_name,
                        account_description,
                        creator_wallet_address,
                        initial_balance,
                    ),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `nextWalletCount` (0x4705161e) function
        pub fn next_wallet_count(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([71, 5, 22, 30], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `owner` (0x8da5cb5b) function
        pub fn owner(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<
            M,
            ::ethers::core::types::Address,
        > {
            self.0
                .method_hash([141, 165, 203, 91], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `registerWalletDerivation` (0xdbb17a0b) function
        pub fn register_wallet_derivation(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            wallet_address_hash: ::ethers::core::types::U256,
            derivation_path: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [219, 177, 122, 11],
                    (
                        account_api_key_hash,
                        wallet_address_hash,
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
                .method_hash(
                    [93, 134, 28, 114],
                    (account_api_key_hash, group_id, action),
                )
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
        ///Calls the contract's `removeWalletFromGroup` (0x6e06ac9c) function
        pub fn remove_wallet_from_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            wallet_address_hash: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [110, 6, 172, 156],
                    (account_api_key_hash, group_id, wallet_address_hash),
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
                    (account_api_key_hash, action_hash, group_id, name, description),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `updateGroup` (0x96a7cc54) function
        pub fn update_group(
            &self,
            account_api_key_hash: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            name: ::std::string::String,
            description: ::std::string::String,
            all_wallets_permitted: bool,
            all_actions_permitted: bool,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash(
                    [150, 167, 204, 84],
                    (
                        account_api_key_hash,
                        group_id,
                        name,
                        description,
                        all_wallets_permitted,
                        all_actions_permitted,
                    ),
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
    for AccountConfig<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
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
        Hash
    )]
    #[etherror(name = "AccountDoesNotExist", abi = "AccountDoesNotExist(uint256)")]
    pub struct AccountDoesNotExist {
        pub api_key: ::ethers::core::types::U256,
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
        Hash
    )]
    #[etherror(
        name = "ActionDoesNotExist",
        abi = "ActionDoesNotExist(uint256,uint256,uint256)"
    )]
    pub struct ActionDoesNotExist {
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub action: ::ethers::core::types::U256,
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
        Hash
    )]
    #[etherror(name = "GroupDoesNotExist", abi = "GroupDoesNotExist(uint256,uint256)")]
    pub struct GroupDoesNotExist {
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
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
        Hash
    )]
    #[etherror(
        name = "UsageApiKeyDoesNotExist",
        abi = "UsageApiKeyDoesNotExist(uint256,uint256)"
    )]
    pub struct UsageApiKeyDoesNotExist {
        pub api_key: ::ethers::core::types::U256,
        pub usage_api_key: ::ethers::core::types::U256,
    }
    ///Custom Error type `WalletDoesNotExist` with signature `WalletDoesNotExist(uint256,uint256,uint256)` and selector `0x79167858`
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
        Hash
    )]
    #[etherror(
        name = "WalletDoesNotExist",
        abi = "WalletDoesNotExist(uint256,uint256,uint256)"
    )]
    pub struct WalletDoesNotExist {
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub wallet: ::ethers::core::types::U256,
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
        Hash
    )]
    pub enum AccountConfigErrors {
        AccountDoesNotExist(AccountDoesNotExist),
        ActionDoesNotExist(ActionDoesNotExist),
        GroupDoesNotExist(GroupDoesNotExist),
        UsageApiKeyDoesNotExist(UsageApiKeyDoesNotExist),
        WalletDoesNotExist(WalletDoesNotExist),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for AccountConfigErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) = <AccountDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AccountDoesNotExist(decoded));
            }
            if let Ok(decoded) = <ActionDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::ActionDoesNotExist(decoded));
            }
            if let Ok(decoded) = <GroupDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::GroupDoesNotExist(decoded));
            }
            if let Ok(decoded) = <UsageApiKeyDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::UsageApiKeyDoesNotExist(decoded));
            }
            if let Ok(decoded) = <WalletDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::WalletDoesNotExist(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for AccountConfigErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::AccountDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ActionDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::GroupDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UsageApiKeyDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::WalletDoesNotExist(element) => {
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
                    == <UsageApiKeyDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <WalletDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for AccountConfigErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AccountDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::ActionDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::GroupDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::UsageApiKeyDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::WalletDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for AccountConfigErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
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
    impl ::core::convert::From<UsageApiKeyDoesNotExist> for AccountConfigErrors {
        fn from(value: UsageApiKeyDoesNotExist) -> Self {
            Self::UsageApiKeyDoesNotExist(value)
        }
    }
    impl ::core::convert::From<WalletDoesNotExist> for AccountConfigErrors {
        fn from(value: WalletDoesNotExist) -> Self {
            Self::WalletDoesNotExist(value)
        }
    }
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
        Hash
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
        Hash
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
    ///Container type for all input parameters for the `addApiKey` function with signature `addApiKey(uint256,uint256,uint256,uint256)` and selector `0x49717935`
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
        Hash
    )]
    #[ethcall(name = "addApiKey", abi = "addApiKey(uint256,uint256,uint256,uint256)")]
    pub struct AddApiKeyCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub usage_api_key_hash: ::ethers::core::types::U256,
        pub expiration: ::ethers::core::types::U256,
        pub balance: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `addGroup` function with signature `addGroup(uint256,string,string,uint256[],uint256[],bool,bool)` and selector `0x749e4d07`
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
        Hash
    )]
    #[ethcall(
        name = "addGroup",
        abi = "addGroup(uint256,string,string,uint256[],uint256[],bool,bool)"
    )]
    pub struct AddGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
        pub permitted_actions: ::std::vec::Vec<::ethers::core::types::U256>,
        pub wallets: ::std::vec::Vec<::ethers::core::types::U256>,
        pub all_wallets_permitted: bool,
        pub all_actions_permitted: bool,
    }
    ///Container type for all input parameters for the `addWalletToGroup` function with signature `addWalletToGroup(uint256,uint256,uint256)` and selector `0xbac710ea`
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
        Hash
    )]
    #[ethcall(
        name = "addWalletToGroup",
        abi = "addWalletToGroup(uint256,uint256,uint256)"
    )]
    pub struct AddWalletToGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub wallet_address_hash: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `getWalletDerivation` function with signature `getWalletDerivation(uint256,uint256)` and selector `0x79b8e691`
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
        Hash
    )]
    #[ethcall(
        name = "getWalletDerivation",
        abi = "getWalletDerivation(uint256,uint256)"
    )]
    pub struct GetWalletDerivationCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub wallet_address_hash: ::ethers::core::types::U256,
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
        Hash
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
        Hash
    )]
    #[ethcall(name = "listApiKeys", abi = "listApiKeys(uint256,uint256,uint256)")]
    pub struct ListApiKeysCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub page_number: ::ethers::core::types::U256,
        pub page_size: ::ethers::core::types::U256,
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
        Hash
    )]
    #[ethcall(name = "listGroups", abi = "listGroups(uint256,uint256,uint256)")]
    pub struct ListGroupsCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub page_number: ::ethers::core::types::U256,
        pub page_size: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `listWallets` function with signature `listWallets(uint256,uint256,uint256)` and selector `0x7af361ef`
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
        Hash
    )]
    #[ethcall(name = "listWallets", abi = "listWallets(uint256,uint256,uint256)")]
    pub struct ListWalletsCall {
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
        Hash
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
    ///Container type for all input parameters for the `newAccount` function with signature `newAccount(uint256,bool,string,string,address,uint256)` and selector `0xbd9aed51`
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
        Hash
    )]
    #[ethcall(
        name = "newAccount",
        abi = "newAccount(uint256,bool,string,string,address,uint256)"
    )]
    pub struct NewAccountCall {
        pub api_key_hash: ::ethers::core::types::U256,
        pub managed: bool,
        pub account_name: ::std::string::String,
        pub account_description: ::std::string::String,
        pub creator_wallet_address: ::ethers::core::types::Address,
        pub initial_balance: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `nextWalletCount` function with signature `nextWalletCount()` and selector `0x4705161e`
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
        Hash
    )]
    #[ethcall(name = "nextWalletCount", abi = "nextWalletCount()")]
    pub struct NextWalletCountCall;
    ///Container type for all input parameters for the `owner` function with signature `owner()` and selector `0x8da5cb5b`
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
        Hash
    )]
    #[ethcall(name = "owner", abi = "owner()")]
    pub struct OwnerCall;
    ///Container type for all input parameters for the `registerWalletDerivation` function with signature `registerWalletDerivation(uint256,uint256,uint256,string,string)` and selector `0xdbb17a0b`
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
        Hash
    )]
    #[ethcall(
        name = "registerWalletDerivation",
        abi = "registerWalletDerivation(uint256,uint256,uint256,string,string)"
    )]
    pub struct RegisterWalletDerivationCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub wallet_address_hash: ::ethers::core::types::U256,
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
        Hash
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
        Hash
    )]
    #[ethcall(name = "removeUsageApiKey", abi = "removeUsageApiKey(uint256,uint256)")]
    pub struct RemoveUsageApiKeyCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub usage_api_key_hash: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `removeWalletFromGroup` function with signature `removeWalletFromGroup(uint256,uint256,uint256)` and selector `0x6e06ac9c`
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
        Hash
    )]
    #[ethcall(
        name = "removeWalletFromGroup",
        abi = "removeWalletFromGroup(uint256,uint256,uint256)"
    )]
    pub struct RemoveWalletFromGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub wallet_address_hash: ::ethers::core::types::U256,
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
        Hash
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
    ///Container type for all input parameters for the `updateGroup` function with signature `updateGroup(uint256,uint256,string,string,bool,bool)` and selector `0x96a7cc54`
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
        Hash
    )]
    #[ethcall(
        name = "updateGroup",
        abi = "updateGroup(uint256,uint256,string,string,bool,bool)"
    )]
    pub struct UpdateGroupCall {
        pub account_api_key_hash: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
        pub all_wallets_permitted: bool,
        pub all_actions_permitted: bool,
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
        Hash
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
        Hash
    )]
    pub enum AccountConfigCalls {
        AccountExistsAndIsMutable(AccountExistsAndIsMutableCall),
        AddActionToGroup(AddActionToGroupCall),
        AddApiKey(AddApiKeyCall),
        AddGroup(AddGroupCall),
        AddWalletToGroup(AddWalletToGroupCall),
        GetWalletDerivation(GetWalletDerivationCall),
        ListActions(ListActionsCall),
        ListApiKeys(ListApiKeysCall),
        ListGroups(ListGroupsCall),
        ListWallets(ListWalletsCall),
        ListWalletsInGroup(ListWalletsInGroupCall),
        NewAccount(NewAccountCall),
        NextWalletCount(NextWalletCountCall),
        Owner(OwnerCall),
        RegisterWalletDerivation(RegisterWalletDerivationCall),
        RemoveActionFromGroup(RemoveActionFromGroupCall),
        RemoveUsageApiKey(RemoveUsageApiKeyCall),
        RemoveWalletFromGroup(RemoveWalletFromGroupCall),
        UpdateActionMetadata(UpdateActionMetadataCall),
        UpdateGroup(UpdateGroupCall),
        UpdateUsageApiKeyMetadata(UpdateUsageApiKeyMetadataCall),
    }
    impl ::ethers::core::abi::AbiDecode for AccountConfigCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <AccountExistsAndIsMutableCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AccountExistsAndIsMutable(decoded));
            }
            if let Ok(decoded) = <AddActionToGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AddActionToGroup(decoded));
            }
            if let Ok(decoded) = <AddApiKeyCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AddApiKey(decoded));
            }
            if let Ok(decoded) = <AddGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AddGroup(decoded));
            }
            if let Ok(decoded) = <AddWalletToGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AddWalletToGroup(decoded));
            }
            if let Ok(decoded) = <GetWalletDerivationCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::GetWalletDerivation(decoded));
            }
            if let Ok(decoded) = <ListActionsCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::ListActions(decoded));
            }
            if let Ok(decoded) = <ListApiKeysCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::ListApiKeys(decoded));
            }
            if let Ok(decoded) = <ListGroupsCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::ListGroups(decoded));
            }
            if let Ok(decoded) = <ListWalletsCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::ListWallets(decoded));
            }
            if let Ok(decoded) = <ListWalletsInGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::ListWalletsInGroup(decoded));
            }
            if let Ok(decoded) = <NewAccountCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::NewAccount(decoded));
            }
            if let Ok(decoded) = <NextWalletCountCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::NextWalletCount(decoded));
            }
            if let Ok(decoded) = <OwnerCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::Owner(decoded));
            }
            if let Ok(decoded) = <RegisterWalletDerivationCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RegisterWalletDerivation(decoded));
            }
            if let Ok(decoded) = <RemoveActionFromGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RemoveActionFromGroup(decoded));
            }
            if let Ok(decoded) = <RemoveUsageApiKeyCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RemoveUsageApiKey(decoded));
            }
            if let Ok(decoded) = <RemoveWalletFromGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RemoveWalletFromGroup(decoded));
            }
            if let Ok(decoded) = <UpdateActionMetadataCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::UpdateActionMetadata(decoded));
            }
            if let Ok(decoded) = <UpdateGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::UpdateGroup(decoded));
            }
            if let Ok(decoded) = <UpdateUsageApiKeyMetadataCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::UpdateUsageApiKeyMetadata(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for AccountConfigCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::AccountExistsAndIsMutable(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddActionToGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddApiKey(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddWalletToGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::GetWalletDerivation(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ListActions(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ListApiKeys(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ListGroups(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ListWallets(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ListWalletsInGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NewAccount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NextWalletCount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::Owner(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RegisterWalletDerivation(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemoveActionFromGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemoveUsageApiKey(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemoveWalletFromGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UpdateActionMetadata(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UpdateGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UpdateUsageApiKeyMetadata(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
            }
        }
    }
    impl ::core::fmt::Display for AccountConfigCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AccountExistsAndIsMutable(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::AddActionToGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddApiKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddWalletToGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetWalletDerivation(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::ListActions(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListApiKeys(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListGroups(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListWallets(element) => ::core::fmt::Display::fmt(element, f),
                Self::ListWalletsInGroup(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::NewAccount(element) => ::core::fmt::Display::fmt(element, f),
                Self::NextWalletCount(element) => ::core::fmt::Display::fmt(element, f),
                Self::Owner(element) => ::core::fmt::Display::fmt(element, f),
                Self::RegisterWalletDerivation(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RemoveActionFromGroup(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RemoveUsageApiKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemoveWalletFromGroup(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::UpdateActionMetadata(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::UpdateGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpdateUsageApiKeyMetadata(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
            }
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
    impl ::core::convert::From<AddApiKeyCall> for AccountConfigCalls {
        fn from(value: AddApiKeyCall) -> Self {
            Self::AddApiKey(value)
        }
    }
    impl ::core::convert::From<AddGroupCall> for AccountConfigCalls {
        fn from(value: AddGroupCall) -> Self {
            Self::AddGroup(value)
        }
    }
    impl ::core::convert::From<AddWalletToGroupCall> for AccountConfigCalls {
        fn from(value: AddWalletToGroupCall) -> Self {
            Self::AddWalletToGroup(value)
        }
    }
    impl ::core::convert::From<GetWalletDerivationCall> for AccountConfigCalls {
        fn from(value: GetWalletDerivationCall) -> Self {
            Self::GetWalletDerivation(value)
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
    impl ::core::convert::From<ListGroupsCall> for AccountConfigCalls {
        fn from(value: ListGroupsCall) -> Self {
            Self::ListGroups(value)
        }
    }
    impl ::core::convert::From<ListWalletsCall> for AccountConfigCalls {
        fn from(value: ListWalletsCall) -> Self {
            Self::ListWallets(value)
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
    impl ::core::convert::From<NextWalletCountCall> for AccountConfigCalls {
        fn from(value: NextWalletCountCall) -> Self {
            Self::NextWalletCount(value)
        }
    }
    impl ::core::convert::From<OwnerCall> for AccountConfigCalls {
        fn from(value: OwnerCall) -> Self {
            Self::Owner(value)
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
    impl ::core::convert::From<RemoveUsageApiKeyCall> for AccountConfigCalls {
        fn from(value: RemoveUsageApiKeyCall) -> Self {
            Self::RemoveUsageApiKey(value)
        }
    }
    impl ::core::convert::From<RemoveWalletFromGroupCall> for AccountConfigCalls {
        fn from(value: RemoveWalletFromGroupCall) -> Self {
            Self::RemoveWalletFromGroup(value)
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
        Hash
    )]
    pub struct AccountExistsAndIsMutableReturn(pub bool);
    ///Container type for all return fields from the `getWalletDerivation` function with signature `getWalletDerivation(uint256,uint256)` and selector `0x79b8e691`
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
        Hash
    )]
    pub struct GetWalletDerivationReturn(pub ::ethers::core::types::U256);
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
        Hash
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
        Hash
    )]
    pub struct ListApiKeysReturn(pub ::std::vec::Vec<UsageApiKey>);
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
        Hash
    )]
    pub struct ListGroupsReturn(pub ::std::vec::Vec<Metadata>);
    ///Container type for all return fields from the `listWallets` function with signature `listWallets(uint256,uint256,uint256)` and selector `0x7af361ef`
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
        Hash
    )]
    pub struct ListWalletsReturn(pub ::std::vec::Vec<Metadata>);
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
        Hash
    )]
    pub struct ListWalletsInGroupReturn(pub ::std::vec::Vec<Metadata>);
    ///Container type for all return fields from the `nextWalletCount` function with signature `nextWalletCount()` and selector `0x4705161e`
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
        Hash
    )]
    pub struct NextWalletCountReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `owner` function with signature `owner()` and selector `0x8da5cb5b`
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
        Hash
    )]
    pub struct OwnerReturn(pub ::ethers::core::types::Address);
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
        Hash
    )]
    pub struct Metadata {
        pub id: ::ethers::core::types::U256,
        pub name: ::std::string::String,
        pub description: ::std::string::String,
    }
    ///`UsageApiKey((uint256,string,string),uint256,uint256,uint256)`
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
        Hash
    )]
    pub struct UsageApiKey {
        pub metadata: Metadata,
        pub api_key_hash: ::ethers::core::types::U256,
        pub expiration: ::ethers::core::types::U256,
        pub balance: ::ethers::core::types::U256,
    }
}
