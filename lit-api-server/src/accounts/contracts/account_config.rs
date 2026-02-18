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
                    ::std::borrow::ToOwned::to_owned("addActionToGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("addActionToGroup"),
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
                                    name: ::std::borrow::ToOwned::to_owned("apiKey"),
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
                                    name: ::std::borrow::ToOwned::to_owned("apiKey"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
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
                                    name: ::std::borrow::ToOwned::to_owned("pkps"),
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
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addPkpToGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("addPkpToGroup"),
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
                                    name: ::std::borrow::ToOwned::to_owned("pkp"),
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
                    ::std::borrow::ToOwned::to_owned("newAccount"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("newAccount"),
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
                                    name: ::std::borrow::ToOwned::to_owned("managed"),
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
                    ::std::borrow::ToOwned::to_owned("nextPkpId"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("nextPkpId"),
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
                    ::std::borrow::ToOwned::to_owned("removeAccountApiKey"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "removeAccountApiKey",
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
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKey"),
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
                    ::std::borrow::ToOwned::to_owned("removeActionFromGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "removeActionFromGroup",
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
                    ::std::borrow::ToOwned::to_owned("removePkpFromGroup"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("removePkpFromGroup"),
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
                                    name: ::std::borrow::ToOwned::to_owned("pkp"),
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
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("AccountApiKeyDoesNotExist"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "AccountApiKeyDoesNotExist",
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
                                    name: ::std::borrow::ToOwned::to_owned("accountApiKey"),
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
                    ::std::borrow::ToOwned::to_owned("PkpDoesNotExist"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("PkpDoesNotExist"),
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
                                    name: ::std::borrow::ToOwned::to_owned("pkp"),
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
    const __BYTECODE: &[u8] = b"`\x80`@R4\x80\x15`\x0EW__\xFD[P_`\x01\x81\x90UPa\x0FY\x80a\0#_9_\xF3\xFE`\x80`@R4\x80\x15a\0\x0FW__\xFD[P`\x046\x10a\0\x91W_5`\xE0\x1C\x80c\xB0XO|\x11a\0dW\x80c\xB0XO|\x14a\x01\x05W\x80c\xC9\xD0\xD7V\x14a\x01!W\x80c\xD2yj\xDF\x14a\x01=W\x80c\xE4<m\x06\x14a\x01YW\x80c\xF03\x9C;\x14a\x01wWa\0\x91V[\x80c\x1FR\xAAB\x14a\0\x95W\x80c]\x86\x1Cr\x14a\0\xB1W\x80c\xA5\x06\xFE\xD0\x14a\0\xCDW\x80c\xAE\xFA\xB5\xAC\x14a\0\xE9W[__\xFD[a\0\xAF`\x04\x806\x03\x81\x01\x90a\0\xAA\x91\x90a\n\xC5V[a\x01\x93V[\0[a\0\xCB`\x04\x806\x03\x81\x01\x90a\0\xC6\x91\x90a\n\xC5V[a\x02\"V[\0[a\0\xE7`\x04\x806\x03\x81\x01\x90a\0\xE2\x91\x90a\n\xC5V[a\x02\xB4V[\0[a\x01\x03`\x04\x806\x03\x81\x01\x90a\0\xFE\x91\x90a\n\xC5V[a\x03CV[\0[a\x01\x1F`\x04\x806\x03\x81\x01\x90a\x01\x1A\x91\x90a\x0CeV[a\x03\xD2V[\0[a\x01;`\x04\x806\x03\x81\x01\x90a\x016\x91\x90a\x0C\xEDV[a\x05*V[\0[a\x01W`\x04\x806\x03\x81\x01\x90a\x01R\x91\x90a\r`V[a\x05\xB9V[\0[a\x01aa\x05\xF4V[`@Qa\x01n\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xF3[a\x01\x91`\x04\x806\x03\x81\x01\x90a\x01\x8C\x91\x90a\n\xC5V[a\x05\xFAV[\0[a\x01\x9D\x83\x83a\x06\x8CV[a\x01\xE0W\x82\x82`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x01\xD7\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x02\x1B\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[a\x02-\x83\x83\x83a\x07\x1DV[a\x02rW\x82\x82\x82`@Q\x7F\xEF%\xD0-\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x02i\x93\x92\x91\x90a\r\xEDV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x02\xAD\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01a\x07\xAB\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[a\x02\xBE\x83\x83a\x06\x8CV[a\x03\x01W\x82\x82`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x02\xF8\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x03<\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[a\x03L\x83a\x07\xC2V[a\x03\x8DW\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x03\x84\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ `\x01\x01_\x85\x81R` \x01\x90\x81R` \x01_ \x90P\x83\x81_\x01\x81\x90UP\x82\x81`\x01\x01\x81\x90UP\x81\x81`\x02\x01\x81\x90UPPPPPV[a\x03\xDB\x83a\x07\xC2V[a\x04\x1CW\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x04\x13\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x04I\x81`\x06\x01T\x82`\x02\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P_\x81`\x04\x01_\x83`\x06\x01T\x81R` \x01\x90\x81R` \x01_ \x90P\x81`\x06\x01T\x81_\x01\x81\x90UP__\x90P[\x84Q\x81\x10\x15a\x04\xBCWa\x04\xAE\x85\x82\x81Q\x81\x10a\x04\x94Wa\x04\x93a\x0E\"V[[` \x02` \x01\x01Q\x83`\x01\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\x04uV[P__\x90P[\x83Q\x81\x10\x15a\x05\tWa\x04\xFB\x84\x82\x81Q\x81\x10a\x04\xE1Wa\x04\xE0a\x0E\"V[[` \x02` \x01\x01Q\x83`\x03\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\x04\xC2V[P\x81`\x06\x01_\x81T\x80\x92\x91\x90a\x05\x1E\x90a\x0E|V[\x91\x90PUPPPPPPV[a\x054\x82\x82a\x07\xDFV[a\x05wW\x81\x81`@Q\x7F\xF4\x11\xAF\xBE\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x05n\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[___\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x80`\x01\x01_\x83\x81R` \x01\x90\x81R` \x01_ __\x82\x01_\x90U`\x01\x82\x01_\x90U`\x02\x82\x01_\x90UPPPPPV[___\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81_\x01\x81\x90UP\x81\x81`\x05\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UPPPPV[`\x01T\x81V[a\x06\x05\x83\x83\x83a\x08YV[a\x06JW\x82\x82\x82`@Q\x7F\xCE\xC3$\xD8\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x06A\x93\x92\x91\x90a\r\xEDV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x06\x85\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x07\xAB\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[_a\x06\x96\x83a\x07\xC2V[a\x06\xD7W\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x06\xCE\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[\x81__\x85\x81R` \x01\x90\x81R` \x01_ `\x04\x01_\x84\x81R` \x01\x90\x81R` \x01_ _\x01T\x14\x90P\x92\x91PPV[_a\x07\x15\x83_\x01\x83_\x1Ba\x08\xE7V[\x90P\x92\x91PPV[_a\x07(\x84\x84a\x06\x8CV[a\x07kW\x83\x83`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x07b\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[a\x07\xA2\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01a\tN\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[_a\x07\xBA\x83_\x01\x83_\x1Ba\teV[\x90P\x92\x91PPV[_\x81__\x84\x81R` \x01\x90\x81R` \x01_ _\x01T\x14\x90P\x91\x90PV[_a\x07\xE9\x83a\x07\xC2V[a\x08*W\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x08!\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[\x81__\x85\x81R` \x01\x90\x81R` \x01_ `\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ _\x01T\x14\x90P\x92\x91PPV[_a\x08d\x84\x84a\x06\x8CV[a\x08\xA7W\x83\x83`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x08\x9E\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[a\x08\xDE\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\tN\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[_a\x08\xF2\x83\x83a\naV[a\tDW\x82_\x01\x82\x90\x80`\x01\x81T\x01\x80\x82U\x80\x91PP`\x01\x90\x03\x90_R` _ \x01_\x90\x91\x90\x91\x90\x91PU\x82_\x01\x80T\x90P\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ \x81\x90UP`\x01\x90Pa\tHV[_\x90P[\x92\x91PPV[_a\t]\x83_\x01\x83_\x1Ba\naV[\x90P\x92\x91PPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x90P_\x81\x14a\nVW_`\x01\x82a\t\x92\x91\x90a\x0E\xC3V[\x90P_`\x01\x86_\x01\x80T\x90Pa\t\xA8\x91\x90a\x0E\xC3V[\x90P\x80\x82\x14a\n\x0EW_\x86_\x01\x82\x81T\x81\x10a\t\xC7Wa\t\xC6a\x0E\"V[[\x90_R` _ \x01T\x90P\x80\x87_\x01\x84\x81T\x81\x10a\t\xE8Wa\t\xE7a\x0E\"V[[\x90_R` _ \x01\x81\x90UP\x83\x87`\x01\x01_\x83\x81R` \x01\x90\x81R` \x01_ \x81\x90UPP[\x85_\x01\x80T\x80a\n!Wa\n a\x0E\xF6V[[`\x01\x90\x03\x81\x81\x90_R` _ \x01_\x90U\x90U\x85`\x01\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x90U`\x01\x93PPPPa\n[V[_\x91PP[\x92\x91PPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x14\x15\x90P\x92\x91PPV[_`@Q\x90P\x90V[__\xFD[__\xFD[_\x81\x90P\x91\x90PV[a\n\xA4\x81a\n\x92V[\x81\x14a\n\xAEW__\xFD[PV[_\x815\x90Pa\n\xBF\x81a\n\x9BV[\x92\x91PPV[___``\x84\x86\x03\x12\x15a\n\xDCWa\n\xDBa\n\x8AV[[_a\n\xE9\x86\x82\x87\x01a\n\xB1V[\x93PP` a\n\xFA\x86\x82\x87\x01a\n\xB1V[\x92PP`@a\x0B\x0B\x86\x82\x87\x01a\n\xB1V[\x91PP\x92P\x92P\x92V[__\xFD[_`\x1F\x19`\x1F\x83\x01\x16\x90P\x91\x90PV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`A`\x04R`$_\xFD[a\x0B_\x82a\x0B\x19V[\x81\x01\x81\x81\x10g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x17\x15a\x0B~Wa\x0B}a\x0B)V[[\x80`@RPPPV[_a\x0B\x90a\n\x81V[\x90Pa\x0B\x9C\x82\x82a\x0BVV[\x91\x90PV[_g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x15a\x0B\xBBWa\x0B\xBAa\x0B)V[[` \x82\x02\x90P` \x81\x01\x90P\x91\x90PV[__\xFD[_a\x0B\xE2a\x0B\xDD\x84a\x0B\xA1V[a\x0B\x87V[\x90P\x80\x83\x82R` \x82\x01\x90P` \x84\x02\x83\x01\x85\x81\x11\x15a\x0C\x05Wa\x0C\x04a\x0B\xCCV[[\x83[\x81\x81\x10\x15a\x0C.W\x80a\x0C\x1A\x88\x82a\n\xB1V[\x84R` \x84\x01\x93PP` \x81\x01\x90Pa\x0C\x07V[PPP\x93\x92PPPV[_\x82`\x1F\x83\x01\x12a\x0CLWa\x0CKa\x0B\x15V[[\x815a\x0C\\\x84\x82` \x86\x01a\x0B\xD0V[\x91PP\x92\x91PPV[___``\x84\x86\x03\x12\x15a\x0C|Wa\x0C{a\n\x8AV[[_a\x0C\x89\x86\x82\x87\x01a\n\xB1V[\x93PP` \x84\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x0C\xAAWa\x0C\xA9a\n\x8EV[[a\x0C\xB6\x86\x82\x87\x01a\x0C8V[\x92PP`@\x84\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x0C\xD7Wa\x0C\xD6a\n\x8EV[[a\x0C\xE3\x86\x82\x87\x01a\x0C8V[\x91PP\x92P\x92P\x92V[__`@\x83\x85\x03\x12\x15a\r\x03Wa\r\x02a\n\x8AV[[_a\r\x10\x85\x82\x86\x01a\n\xB1V[\x92PP` a\r!\x85\x82\x86\x01a\n\xB1V[\x91PP\x92P\x92\x90PV[_\x81\x15\x15\x90P\x91\x90PV[a\r?\x81a\r+V[\x81\x14a\rIW__\xFD[PV[_\x815\x90Pa\rZ\x81a\r6V[\x92\x91PPV[__`@\x83\x85\x03\x12\x15a\rvWa\rua\n\x8AV[[_a\r\x83\x85\x82\x86\x01a\n\xB1V[\x92PP` a\r\x94\x85\x82\x86\x01a\rLV[\x91PP\x92P\x92\x90PV[a\r\xA7\x81a\n\x92V[\x82RPPV[_` \x82\x01\x90Pa\r\xC0_\x83\x01\x84a\r\x9EV[\x92\x91PPV[_`@\x82\x01\x90Pa\r\xD9_\x83\x01\x85a\r\x9EV[a\r\xE6` \x83\x01\x84a\r\x9EV[\x93\x92PPPV[_``\x82\x01\x90Pa\x0E\0_\x83\x01\x86a\r\x9EV[a\x0E\r` \x83\x01\x85a\r\x9EV[a\x0E\x1A`@\x83\x01\x84a\r\x9EV[\x94\x93PPPPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`2`\x04R`$_\xFD[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`\x11`\x04R`$_\xFD[_a\x0E\x86\x82a\n\x92V[\x91P\x7F\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x03a\x0E\xB8Wa\x0E\xB7a\x0EOV[[`\x01\x82\x01\x90P\x91\x90PV[_a\x0E\xCD\x82a\n\x92V[\x91Pa\x0E\xD8\x83a\n\x92V[\x92P\x82\x82\x03\x90P\x81\x81\x11\x15a\x0E\xF0Wa\x0E\xEFa\x0EOV[[\x92\x91PPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`1`\x04R`$_\xFD\xFE\xA2dipfsX\"\x12 0\x08rE8\x16rU,\x97\x9Ds;.u\x96\xCB\x96\xD0\xFF\x80\xC3\x8Ds\x12\x11\x03Xo\xE4\xDF\xABdsolcC\0\x08\x1C\x003";
    /// The bytecode of the contract.
    pub static ACCOUNTCONFIG_BYTECODE: ::ethers::core::types::Bytes = ::ethers::core::types::Bytes::from_static(
        __BYTECODE,
    );
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R4\x80\x15a\0\x0FW__\xFD[P`\x046\x10a\0\x91W_5`\xE0\x1C\x80c\xB0XO|\x11a\0dW\x80c\xB0XO|\x14a\x01\x05W\x80c\xC9\xD0\xD7V\x14a\x01!W\x80c\xD2yj\xDF\x14a\x01=W\x80c\xE4<m\x06\x14a\x01YW\x80c\xF03\x9C;\x14a\x01wWa\0\x91V[\x80c\x1FR\xAAB\x14a\0\x95W\x80c]\x86\x1Cr\x14a\0\xB1W\x80c\xA5\x06\xFE\xD0\x14a\0\xCDW\x80c\xAE\xFA\xB5\xAC\x14a\0\xE9W[__\xFD[a\0\xAF`\x04\x806\x03\x81\x01\x90a\0\xAA\x91\x90a\n\xC5V[a\x01\x93V[\0[a\0\xCB`\x04\x806\x03\x81\x01\x90a\0\xC6\x91\x90a\n\xC5V[a\x02\"V[\0[a\0\xE7`\x04\x806\x03\x81\x01\x90a\0\xE2\x91\x90a\n\xC5V[a\x02\xB4V[\0[a\x01\x03`\x04\x806\x03\x81\x01\x90a\0\xFE\x91\x90a\n\xC5V[a\x03CV[\0[a\x01\x1F`\x04\x806\x03\x81\x01\x90a\x01\x1A\x91\x90a\x0CeV[a\x03\xD2V[\0[a\x01;`\x04\x806\x03\x81\x01\x90a\x016\x91\x90a\x0C\xEDV[a\x05*V[\0[a\x01W`\x04\x806\x03\x81\x01\x90a\x01R\x91\x90a\r`V[a\x05\xB9V[\0[a\x01aa\x05\xF4V[`@Qa\x01n\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xF3[a\x01\x91`\x04\x806\x03\x81\x01\x90a\x01\x8C\x91\x90a\n\xC5V[a\x05\xFAV[\0[a\x01\x9D\x83\x83a\x06\x8CV[a\x01\xE0W\x82\x82`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x01\xD7\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x02\x1B\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[a\x02-\x83\x83\x83a\x07\x1DV[a\x02rW\x82\x82\x82`@Q\x7F\xEF%\xD0-\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x02i\x93\x92\x91\x90a\r\xEDV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x02\xAD\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01a\x07\xAB\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[a\x02\xBE\x83\x83a\x06\x8CV[a\x03\x01W\x82\x82`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x02\xF8\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x03<\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[a\x03L\x83a\x07\xC2V[a\x03\x8DW\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x03\x84\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ `\x01\x01_\x85\x81R` \x01\x90\x81R` \x01_ \x90P\x83\x81_\x01\x81\x90UP\x82\x81`\x01\x01\x81\x90UP\x81\x81`\x02\x01\x81\x90UPPPPPV[a\x03\xDB\x83a\x07\xC2V[a\x04\x1CW\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x04\x13\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x04I\x81`\x06\x01T\x82`\x02\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P_\x81`\x04\x01_\x83`\x06\x01T\x81R` \x01\x90\x81R` \x01_ \x90P\x81`\x06\x01T\x81_\x01\x81\x90UP__\x90P[\x84Q\x81\x10\x15a\x04\xBCWa\x04\xAE\x85\x82\x81Q\x81\x10a\x04\x94Wa\x04\x93a\x0E\"V[[` \x02` \x01\x01Q\x83`\x01\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\x04uV[P__\x90P[\x83Q\x81\x10\x15a\x05\tWa\x04\xFB\x84\x82\x81Q\x81\x10a\x04\xE1Wa\x04\xE0a\x0E\"V[[` \x02` \x01\x01Q\x83`\x03\x01a\x07\x06\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[P\x80\x80`\x01\x01\x91PPa\x04\xC2V[P\x81`\x06\x01_\x81T\x80\x92\x91\x90a\x05\x1E\x90a\x0E|V[\x91\x90PUPPPPPPV[a\x054\x82\x82a\x07\xDFV[a\x05wW\x81\x81`@Q\x7F\xF4\x11\xAF\xBE\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x05n\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[___\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x80`\x01\x01_\x83\x81R` \x01\x90\x81R` \x01_ __\x82\x01_\x90U`\x01\x82\x01_\x90U`\x02\x82\x01_\x90UPPPPPV[___\x84\x81R` \x01\x90\x81R` \x01_ \x90P\x82\x81_\x01\x81\x90UP\x81\x81`\x05\x01_a\x01\0\n\x81T\x81`\xFF\x02\x19\x16\x90\x83\x15\x15\x02\x17\x90UPPPPV[`\x01T\x81V[a\x06\x05\x83\x83\x83a\x08YV[a\x06JW\x82\x82\x82`@Q\x7F\xCE\xC3$\xD8\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x06A\x93\x92\x91\x90a\r\xEDV[`@Q\x80\x91\x03\x90\xFD[___\x85\x81R` \x01\x90\x81R` \x01_ \x90Pa\x06\x85\x82\x82`\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\x07\xAB\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[PPPPPV[_a\x06\x96\x83a\x07\xC2V[a\x06\xD7W\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x06\xCE\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[\x81__\x85\x81R` \x01\x90\x81R` \x01_ `\x04\x01_\x84\x81R` \x01\x90\x81R` \x01_ _\x01T\x14\x90P\x92\x91PPV[_a\x07\x15\x83_\x01\x83_\x1Ba\x08\xE7V[\x90P\x92\x91PPV[_a\x07(\x84\x84a\x06\x8CV[a\x07kW\x83\x83`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x07b\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[a\x07\xA2\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x01\x01a\tN\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[_a\x07\xBA\x83_\x01\x83_\x1Ba\teV[\x90P\x92\x91PPV[_\x81__\x84\x81R` \x01\x90\x81R` \x01_ _\x01T\x14\x90P\x91\x90PV[_a\x07\xE9\x83a\x07\xC2V[a\x08*W\x82`@Q\x7F\xD4\xA8G7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x08!\x91\x90a\r\xADV[`@Q\x80\x91\x03\x90\xFD[\x81__\x85\x81R` \x01\x90\x81R` \x01_ `\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ _\x01T\x14\x90P\x92\x91PPV[_a\x08d\x84\x84a\x06\x8CV[a\x08\xA7W\x83\x83`@Q\x7F\x93\x1A\x85\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81R`\x04\x01a\x08\x9E\x92\x91\x90a\r\xC6V[`@Q\x80\x91\x03\x90\xFD[a\x08\xDE\x82__\x87\x81R` \x01\x90\x81R` \x01_ `\x04\x01_\x86\x81R` \x01\x90\x81R` \x01_ `\x03\x01a\tN\x90\x91\x90c\xFF\xFF\xFF\xFF\x16V[\x90P\x93\x92PPPV[_a\x08\xF2\x83\x83a\naV[a\tDW\x82_\x01\x82\x90\x80`\x01\x81T\x01\x80\x82U\x80\x91PP`\x01\x90\x03\x90_R` _ \x01_\x90\x91\x90\x91\x90\x91PU\x82_\x01\x80T\x90P\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ \x81\x90UP`\x01\x90Pa\tHV[_\x90P[\x92\x91PPV[_a\t]\x83_\x01\x83_\x1Ba\naV[\x90P\x92\x91PPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x90P_\x81\x14a\nVW_`\x01\x82a\t\x92\x91\x90a\x0E\xC3V[\x90P_`\x01\x86_\x01\x80T\x90Pa\t\xA8\x91\x90a\x0E\xC3V[\x90P\x80\x82\x14a\n\x0EW_\x86_\x01\x82\x81T\x81\x10a\t\xC7Wa\t\xC6a\x0E\"V[[\x90_R` _ \x01T\x90P\x80\x87_\x01\x84\x81T\x81\x10a\t\xE8Wa\t\xE7a\x0E\"V[[\x90_R` _ \x01\x81\x90UP\x83\x87`\x01\x01_\x83\x81R` \x01\x90\x81R` \x01_ \x81\x90UPP[\x85_\x01\x80T\x80a\n!Wa\n a\x0E\xF6V[[`\x01\x90\x03\x81\x81\x90_R` _ \x01_\x90U\x90U\x85`\x01\x01_\x86\x81R` \x01\x90\x81R` \x01_ _\x90U`\x01\x93PPPPa\n[V[_\x91PP[\x92\x91PPV[__\x83`\x01\x01_\x84\x81R` \x01\x90\x81R` \x01_ T\x14\x15\x90P\x92\x91PPV[_`@Q\x90P\x90V[__\xFD[__\xFD[_\x81\x90P\x91\x90PV[a\n\xA4\x81a\n\x92V[\x81\x14a\n\xAEW__\xFD[PV[_\x815\x90Pa\n\xBF\x81a\n\x9BV[\x92\x91PPV[___``\x84\x86\x03\x12\x15a\n\xDCWa\n\xDBa\n\x8AV[[_a\n\xE9\x86\x82\x87\x01a\n\xB1V[\x93PP` a\n\xFA\x86\x82\x87\x01a\n\xB1V[\x92PP`@a\x0B\x0B\x86\x82\x87\x01a\n\xB1V[\x91PP\x92P\x92P\x92V[__\xFD[_`\x1F\x19`\x1F\x83\x01\x16\x90P\x91\x90PV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`A`\x04R`$_\xFD[a\x0B_\x82a\x0B\x19V[\x81\x01\x81\x81\x10g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x17\x15a\x0B~Wa\x0B}a\x0B)V[[\x80`@RPPPV[_a\x0B\x90a\n\x81V[\x90Pa\x0B\x9C\x82\x82a\x0BVV[\x91\x90PV[_g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x11\x15a\x0B\xBBWa\x0B\xBAa\x0B)V[[` \x82\x02\x90P` \x81\x01\x90P\x91\x90PV[__\xFD[_a\x0B\xE2a\x0B\xDD\x84a\x0B\xA1V[a\x0B\x87V[\x90P\x80\x83\x82R` \x82\x01\x90P` \x84\x02\x83\x01\x85\x81\x11\x15a\x0C\x05Wa\x0C\x04a\x0B\xCCV[[\x83[\x81\x81\x10\x15a\x0C.W\x80a\x0C\x1A\x88\x82a\n\xB1V[\x84R` \x84\x01\x93PP` \x81\x01\x90Pa\x0C\x07V[PPP\x93\x92PPPV[_\x82`\x1F\x83\x01\x12a\x0CLWa\x0CKa\x0B\x15V[[\x815a\x0C\\\x84\x82` \x86\x01a\x0B\xD0V[\x91PP\x92\x91PPV[___``\x84\x86\x03\x12\x15a\x0C|Wa\x0C{a\n\x8AV[[_a\x0C\x89\x86\x82\x87\x01a\n\xB1V[\x93PP` \x84\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x0C\xAAWa\x0C\xA9a\n\x8EV[[a\x0C\xB6\x86\x82\x87\x01a\x0C8V[\x92PP`@\x84\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x0C\xD7Wa\x0C\xD6a\n\x8EV[[a\x0C\xE3\x86\x82\x87\x01a\x0C8V[\x91PP\x92P\x92P\x92V[__`@\x83\x85\x03\x12\x15a\r\x03Wa\r\x02a\n\x8AV[[_a\r\x10\x85\x82\x86\x01a\n\xB1V[\x92PP` a\r!\x85\x82\x86\x01a\n\xB1V[\x91PP\x92P\x92\x90PV[_\x81\x15\x15\x90P\x91\x90PV[a\r?\x81a\r+V[\x81\x14a\rIW__\xFD[PV[_\x815\x90Pa\rZ\x81a\r6V[\x92\x91PPV[__`@\x83\x85\x03\x12\x15a\rvWa\rua\n\x8AV[[_a\r\x83\x85\x82\x86\x01a\n\xB1V[\x92PP` a\r\x94\x85\x82\x86\x01a\rLV[\x91PP\x92P\x92\x90PV[a\r\xA7\x81a\n\x92V[\x82RPPV[_` \x82\x01\x90Pa\r\xC0_\x83\x01\x84a\r\x9EV[\x92\x91PPV[_`@\x82\x01\x90Pa\r\xD9_\x83\x01\x85a\r\x9EV[a\r\xE6` \x83\x01\x84a\r\x9EV[\x93\x92PPPV[_``\x82\x01\x90Pa\x0E\0_\x83\x01\x86a\r\x9EV[a\x0E\r` \x83\x01\x85a\r\x9EV[a\x0E\x1A`@\x83\x01\x84a\r\x9EV[\x94\x93PPPPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`2`\x04R`$_\xFD[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`\x11`\x04R`$_\xFD[_a\x0E\x86\x82a\n\x92V[\x91P\x7F\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x82\x03a\x0E\xB8Wa\x0E\xB7a\x0EOV[[`\x01\x82\x01\x90P\x91\x90PV[_a\x0E\xCD\x82a\n\x92V[\x91Pa\x0E\xD8\x83a\n\x92V[\x92P\x82\x82\x03\x90P\x81\x81\x11\x15a\x0E\xF0Wa\x0E\xEFa\x0EOV[[\x92\x91PPV[\x7FNH{q\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0_R`1`\x04R`$_\xFD\xFE\xA2dipfsX\"\x12 0\x08rE8\x16rU,\x97\x9Ds;.u\x96\xCB\x96\xD0\xFF\x80\xC3\x8Ds\x12\x11\x03Xo\xE4\xDF\xABdsolcC\0\x08\x1C\x003";
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
        ///Calls the contract's `addActionToGroup` (0x1f52aa42) function
        pub fn add_action_to_group(
            &self,
            api_key: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            action: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([31, 82, 170, 66], (api_key, group_id, action))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addApiKey` (0xaefab5ac) function
        pub fn add_api_key(
            &self,
            api_key: ::ethers::core::types::U256,
            expiration: ::ethers::core::types::U256,
            balance: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([174, 250, 181, 172], (api_key, expiration, balance))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addGroup` (0xb0584f7c) function
        pub fn add_group(
            &self,
            api_key: ::ethers::core::types::U256,
            permitted_actions: ::std::vec::Vec<::ethers::core::types::U256>,
            pkps: ::std::vec::Vec<::ethers::core::types::U256>,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([176, 88, 79, 124], (api_key, permitted_actions, pkps))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addPkpToGroup` (0xa506fed0) function
        pub fn add_pkp_to_group(
            &self,
            api_key: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            pkp: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([165, 6, 254, 208], (api_key, group_id, pkp))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `newAccount` (0xd2796adf) function
        pub fn new_account(
            &self,
            api_key: ::ethers::core::types::U256,
            managed: bool,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([210, 121, 106, 223], (api_key, managed))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `nextPkpId` (0xe43c6d06) function
        pub fn next_pkp_id(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([228, 60, 109, 6], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `removeAccountApiKey` (0xc9d0d756) function
        pub fn remove_account_api_key(
            &self,
            api_key: ::ethers::core::types::U256,
            account_api_key: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([201, 208, 215, 86], (api_key, account_api_key))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `removeActionFromGroup` (0x5d861c72) function
        pub fn remove_action_from_group(
            &self,
            api_key: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            action: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([93, 134, 28, 114], (api_key, group_id, action))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `removePkpFromGroup` (0xf0339c3b) function
        pub fn remove_pkp_from_group(
            &self,
            api_key: ::ethers::core::types::U256,
            group_id: ::ethers::core::types::U256,
            pkp: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([240, 51, 156, 59], (api_key, group_id, pkp))
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
    for AccountConfig<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `AccountApiKeyDoesNotExist` with signature `AccountApiKeyDoesNotExist(uint256,uint256)` and selector `0xf411afbe`
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
        name = "AccountApiKeyDoesNotExist",
        abi = "AccountApiKeyDoesNotExist(uint256,uint256)"
    )]
    pub struct AccountApiKeyDoesNotExist {
        pub api_key: ::ethers::core::types::U256,
        pub account_api_key: ::ethers::core::types::U256,
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
    ///Custom Error type `PkpDoesNotExist` with signature `PkpDoesNotExist(uint256,uint256,uint256)` and selector `0xcec324d8`
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
        name = "PkpDoesNotExist",
        abi = "PkpDoesNotExist(uint256,uint256,uint256)"
    )]
    pub struct PkpDoesNotExist {
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub pkp: ::ethers::core::types::U256,
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
        AccountApiKeyDoesNotExist(AccountApiKeyDoesNotExist),
        AccountDoesNotExist(AccountDoesNotExist),
        ActionDoesNotExist(ActionDoesNotExist),
        GroupDoesNotExist(GroupDoesNotExist),
        PkpDoesNotExist(PkpDoesNotExist),
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
            if let Ok(decoded) = <AccountApiKeyDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AccountApiKeyDoesNotExist(decoded));
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
            if let Ok(decoded) = <PkpDoesNotExist as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::PkpDoesNotExist(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for AccountConfigErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::AccountApiKeyDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AccountDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ActionDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::GroupDoesNotExist(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::PkpDoesNotExist(element) => {
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
                    == <AccountApiKeyDoesNotExist as ::ethers::contract::EthError>::selector() => {
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
                    == <PkpDoesNotExist as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for AccountConfigErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AccountApiKeyDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::AccountDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::ActionDoesNotExist(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::GroupDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::PkpDoesNotExist(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for AccountConfigErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<AccountApiKeyDoesNotExist> for AccountConfigErrors {
        fn from(value: AccountApiKeyDoesNotExist) -> Self {
            Self::AccountApiKeyDoesNotExist(value)
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
    impl ::core::convert::From<PkpDoesNotExist> for AccountConfigErrors {
        fn from(value: PkpDoesNotExist) -> Self {
            Self::PkpDoesNotExist(value)
        }
    }
    ///Container type for all input parameters for the `addActionToGroup` function with signature `addActionToGroup(uint256,uint256,uint256)` and selector `0x1f52aa42`
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
        abi = "addActionToGroup(uint256,uint256,uint256)"
    )]
    pub struct AddActionToGroupCall {
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub action: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `addApiKey` function with signature `addApiKey(uint256,uint256,uint256)` and selector `0xaefab5ac`
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
    #[ethcall(name = "addApiKey", abi = "addApiKey(uint256,uint256,uint256)")]
    pub struct AddApiKeyCall {
        pub api_key: ::ethers::core::types::U256,
        pub expiration: ::ethers::core::types::U256,
        pub balance: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `addGroup` function with signature `addGroup(uint256,uint256[],uint256[])` and selector `0xb0584f7c`
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
    #[ethcall(name = "addGroup", abi = "addGroup(uint256,uint256[],uint256[])")]
    pub struct AddGroupCall {
        pub api_key: ::ethers::core::types::U256,
        pub permitted_actions: ::std::vec::Vec<::ethers::core::types::U256>,
        pub pkps: ::std::vec::Vec<::ethers::core::types::U256>,
    }
    ///Container type for all input parameters for the `addPkpToGroup` function with signature `addPkpToGroup(uint256,uint256,uint256)` and selector `0xa506fed0`
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
    #[ethcall(name = "addPkpToGroup", abi = "addPkpToGroup(uint256,uint256,uint256)")]
    pub struct AddPkpToGroupCall {
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub pkp: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `newAccount` function with signature `newAccount(uint256,bool)` and selector `0xd2796adf`
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
    #[ethcall(name = "newAccount", abi = "newAccount(uint256,bool)")]
    pub struct NewAccountCall {
        pub api_key: ::ethers::core::types::U256,
        pub managed: bool,
    }
    ///Container type for all input parameters for the `nextPkpId` function with signature `nextPkpId()` and selector `0xe43c6d06`
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
    #[ethcall(name = "nextPkpId", abi = "nextPkpId()")]
    pub struct NextPkpIdCall;
    ///Container type for all input parameters for the `removeAccountApiKey` function with signature `removeAccountApiKey(uint256,uint256)` and selector `0xc9d0d756`
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
        name = "removeAccountApiKey",
        abi = "removeAccountApiKey(uint256,uint256)"
    )]
    pub struct RemoveAccountApiKeyCall {
        pub api_key: ::ethers::core::types::U256,
        pub account_api_key: ::ethers::core::types::U256,
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
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub action: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `removePkpFromGroup` function with signature `removePkpFromGroup(uint256,uint256,uint256)` and selector `0xf0339c3b`
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
        name = "removePkpFromGroup",
        abi = "removePkpFromGroup(uint256,uint256,uint256)"
    )]
    pub struct RemovePkpFromGroupCall {
        pub api_key: ::ethers::core::types::U256,
        pub group_id: ::ethers::core::types::U256,
        pub pkp: ::ethers::core::types::U256,
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
        AddActionToGroup(AddActionToGroupCall),
        AddApiKey(AddApiKeyCall),
        AddGroup(AddGroupCall),
        AddPkpToGroup(AddPkpToGroupCall),
        NewAccount(NewAccountCall),
        NextPkpId(NextPkpIdCall),
        RemoveAccountApiKey(RemoveAccountApiKeyCall),
        RemoveActionFromGroup(RemoveActionFromGroupCall),
        RemovePkpFromGroup(RemovePkpFromGroupCall),
    }
    impl ::ethers::core::abi::AbiDecode for AccountConfigCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
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
            if let Ok(decoded) = <AddPkpToGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AddPkpToGroup(decoded));
            }
            if let Ok(decoded) = <NewAccountCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::NewAccount(decoded));
            }
            if let Ok(decoded) = <NextPkpIdCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::NextPkpId(decoded));
            }
            if let Ok(decoded) = <RemoveAccountApiKeyCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RemoveAccountApiKey(decoded));
            }
            if let Ok(decoded) = <RemoveActionFromGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RemoveActionFromGroup(decoded));
            }
            if let Ok(decoded) = <RemovePkpFromGroupCall as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RemovePkpFromGroup(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for AccountConfigCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::AddActionToGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddApiKey(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::AddPkpToGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NewAccount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NextPkpId(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemoveAccountApiKey(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemoveActionFromGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RemovePkpFromGroup(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
            }
        }
    }
    impl ::core::fmt::Display for AccountConfigCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AddActionToGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddApiKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddPkpToGroup(element) => ::core::fmt::Display::fmt(element, f),
                Self::NewAccount(element) => ::core::fmt::Display::fmt(element, f),
                Self::NextPkpId(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemoveAccountApiKey(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RemoveActionFromGroup(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RemovePkpFromGroup(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
            }
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
    impl ::core::convert::From<AddPkpToGroupCall> for AccountConfigCalls {
        fn from(value: AddPkpToGroupCall) -> Self {
            Self::AddPkpToGroup(value)
        }
    }
    impl ::core::convert::From<NewAccountCall> for AccountConfigCalls {
        fn from(value: NewAccountCall) -> Self {
            Self::NewAccount(value)
        }
    }
    impl ::core::convert::From<NextPkpIdCall> for AccountConfigCalls {
        fn from(value: NextPkpIdCall) -> Self {
            Self::NextPkpId(value)
        }
    }
    impl ::core::convert::From<RemoveAccountApiKeyCall> for AccountConfigCalls {
        fn from(value: RemoveAccountApiKeyCall) -> Self {
            Self::RemoveAccountApiKey(value)
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
    ///Container type for all return fields from the `nextPkpId` function with signature `nextPkpId()` and selector `0xe43c6d06`
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
    pub struct NextPkpIdReturn(pub ::ethers::core::types::U256);
}
