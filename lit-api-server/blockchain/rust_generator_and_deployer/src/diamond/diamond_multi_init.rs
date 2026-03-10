pub use diamond_multi_init::*;
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
pub mod diamond_multi_init {
    const _: () = {
        ::core::include_bytes!(
            "./DiamondMultiInit.json",
        );
    };
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("multiInit"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("multiInit"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_addresses"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Address,
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address[]"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_calldata"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes[]"),
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
                    ::std::borrow::ToOwned::to_owned(
                        "AddressAndCalldataLengthDoNotMatch",
                    ),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "AddressAndCalldataLengthDoNotMatch",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_addressesLength"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_calldataLength"),
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
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static DIAMONDMULTIINIT_ABI: ::ethers::contract::Lazy<
        ::ethers::core::abi::Abi,
    > = ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80\x80`@R4`\x15Wa\x03m\x90\x81a\0\x1B\x829\xF3[`\0\x80\xFD\xFE`\x80`@R`\x046\x10\x15a\0\x12W`\0\x80\xFD[`\x005`\xE0\x1Ccn\x02\xFA<\x14a\0'W`\0\x80\xFD[4a\x01]W`@6`\x03\x19\x01\x12a\x01]W`\x045g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01]Wa\0X\x906\x90`\x04\x01a\x01bV[`$5g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01]Wa\0x\x906\x90`\x04\x01a\x01bV[\x90\x91\x81\x81\x03a\x01FW\x916\x81\x90\x03`\x1E\x19\x01\x90`\0[\x84\x81\x10\x15a\x01DW`\0`\x05\x82\x90\x1B\x87\x81\x015\x90`\x01`\x01`\xA0\x1B\x03\x82\x16\x82\x03a\x01(W\x86\x84\x10\x15a\x010W\x84\x015\x85\x81\x12\x15a\x01(W\x84\x01\x91\x825g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01,W` \x84\x01\x816\x03\x81\x13a\x01(Wa\0\xF7a\0\xF2\x83a\x01\xCFV[a\x01\x93V[\x94\x82\x86R` \x836\x92\x01\x01\x11a\x01(W\x91` \x82a\x01\"\x95\x93\x87\x95\x83`\x01\x9A\x99\x017\x84\x01\x01Ra\x02,V[\x01a\0\x8EV[\x82\x80\xFD[P\x80\xFD[cNH{q`\xE0\x1B\x83R`2`\x04R`$\x83\xFD[\0[c9\xA0p\xB3`\xE1\x1B`\0R`\x04R`$R`D`\0\xFD[`\0\x80\xFD[\x91\x81`\x1F\x84\x01\x12\x15a\x01]W\x825\x91g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x83\x11a\x01]W` \x80\x85\x01\x94\x84`\x05\x1B\x01\x01\x11a\x01]WV[`@Q\x91\x90`\x1F\x01`\x1F\x19\x16\x82\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x83\x82\x10\x17a\x01\xB9W`@RV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01\xB9W`\x1F\x01`\x1F\x19\x16` \x01\x90V[\x91\x90\x82Q\x92\x83\x82R`\0[\x84\x81\x10a\x02\x17WPP\x82`\0` \x80\x94\x95\x84\x01\x01R`\x1F\x80\x19\x91\x01\x16\x01\x01\x90V[\x80` \x80\x92\x84\x01\x01Q\x82\x82\x86\x01\x01R\x01a\x01\xF6V[`\x01`\x01`\xA0\x1B\x03\x81\x16\x91\x90\x82\x15a\x032Wa\x02H``a\x01\x93V[`(\x81R\x7FLibDiamondCut: _init address has` \x82\x01Rg no code`\xC0\x1B`@\x82\x01R\x81;\x15a\x03\x08WP`\0\x80\x91\x83Q\x90` \x85\x01\x90Z\xF4\x91=\x15a\x03\0W=\x92a\x02\xAEa\0\xF2\x85a\x01\xCFV[\x93\x84R=`\0` \x86\x01>[\x15a\x02\xC4WPPPV[\x82Q\x15a\x02\xD3W\x82Q` \x84\x01\xFD[a\x02\xFC`@Q\x92\x83\x92c\x19!\x05\xD7`\xE0\x1B\x84R`\x04\x84\x01R`@`$\x84\x01R`D\x83\x01\x90a\x01\xEBV[\x03\x90\xFD[``\x92a\x02\xBAV[\x83a\x02\xFC`@Q\x92\x83\x92c\x91\x984\xB9`\xE0\x1B\x84R`\x04\x84\x01R`@`$\x84\x01R`D\x83\x01\x90a\x01\xEBV[PPPV\xFE\xA2dipfsX\"\x12 \xEBID\xA8\x06U\x13\xF6\x19J\xD4\xE4\xE0Rztt\xCA\x92\xC53\x85\x1C\x0BC{`\xE8\x12\xEA\0\x1BdsolcC\0\x08\x1C\x003";
    /// The bytecode of the contract.
    pub static DIAMONDMULTIINIT_BYTECODE: ::ethers::core::types::Bytes = ::ethers::core::types::Bytes::from_static(
        __BYTECODE,
    );
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\x046\x10\x15a\0\x12W`\0\x80\xFD[`\x005`\xE0\x1Ccn\x02\xFA<\x14a\0'W`\0\x80\xFD[4a\x01]W`@6`\x03\x19\x01\x12a\x01]W`\x045g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01]Wa\0X\x906\x90`\x04\x01a\x01bV[`$5g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01]Wa\0x\x906\x90`\x04\x01a\x01bV[\x90\x91\x81\x81\x03a\x01FW\x916\x81\x90\x03`\x1E\x19\x01\x90`\0[\x84\x81\x10\x15a\x01DW`\0`\x05\x82\x90\x1B\x87\x81\x015\x90`\x01`\x01`\xA0\x1B\x03\x82\x16\x82\x03a\x01(W\x86\x84\x10\x15a\x010W\x84\x015\x85\x81\x12\x15a\x01(W\x84\x01\x91\x825g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01,W` \x84\x01\x816\x03\x81\x13a\x01(Wa\0\xF7a\0\xF2\x83a\x01\xCFV[a\x01\x93V[\x94\x82\x86R` \x836\x92\x01\x01\x11a\x01(W\x91` \x82a\x01\"\x95\x93\x87\x95\x83`\x01\x9A\x99\x017\x84\x01\x01Ra\x02,V[\x01a\0\x8EV[\x82\x80\xFD[P\x80\xFD[cNH{q`\xE0\x1B\x83R`2`\x04R`$\x83\xFD[\0[c9\xA0p\xB3`\xE1\x1B`\0R`\x04R`$R`D`\0\xFD[`\0\x80\xFD[\x91\x81`\x1F\x84\x01\x12\x15a\x01]W\x825\x91g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x83\x11a\x01]W` \x80\x85\x01\x94\x84`\x05\x1B\x01\x01\x11a\x01]WV[`@Q\x91\x90`\x1F\x01`\x1F\x19\x16\x82\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x83\x82\x10\x17a\x01\xB9W`@RV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11a\x01\xB9W`\x1F\x01`\x1F\x19\x16` \x01\x90V[\x91\x90\x82Q\x92\x83\x82R`\0[\x84\x81\x10a\x02\x17WPP\x82`\0` \x80\x94\x95\x84\x01\x01R`\x1F\x80\x19\x91\x01\x16\x01\x01\x90V[\x80` \x80\x92\x84\x01\x01Q\x82\x82\x86\x01\x01R\x01a\x01\xF6V[`\x01`\x01`\xA0\x1B\x03\x81\x16\x91\x90\x82\x15a\x032Wa\x02H``a\x01\x93V[`(\x81R\x7FLibDiamondCut: _init address has` \x82\x01Rg no code`\xC0\x1B`@\x82\x01R\x81;\x15a\x03\x08WP`\0\x80\x91\x83Q\x90` \x85\x01\x90Z\xF4\x91=\x15a\x03\0W=\x92a\x02\xAEa\0\xF2\x85a\x01\xCFV[\x93\x84R=`\0` \x86\x01>[\x15a\x02\xC4WPPPV[\x82Q\x15a\x02\xD3W\x82Q` \x84\x01\xFD[a\x02\xFC`@Q\x92\x83\x92c\x19!\x05\xD7`\xE0\x1B\x84R`\x04\x84\x01R`@`$\x84\x01R`D\x83\x01\x90a\x01\xEBV[\x03\x90\xFD[``\x92a\x02\xBAV[\x83a\x02\xFC`@Q\x92\x83\x92c\x91\x984\xB9`\xE0\x1B\x84R`\x04\x84\x01R`@`$\x84\x01R`D\x83\x01\x90a\x01\xEBV[PPPV\xFE\xA2dipfsX\"\x12 \xEBID\xA8\x06U\x13\xF6\x19J\xD4\xE4\xE0Rztt\xCA\x92\xC53\x85\x1C\x0BC{`\xE8\x12\xEA\0\x1BdsolcC\0\x08\x1C\x003";
    /// The deployed bytecode of the contract.
    pub static DIAMONDMULTIINIT_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes = ::ethers::core::types::Bytes::from_static(
        __DEPLOYED_BYTECODE,
    );
    pub struct DiamondMultiInit<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for DiamondMultiInit<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for DiamondMultiInit<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for DiamondMultiInit<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for DiamondMultiInit<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(DiamondMultiInit))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> DiamondMultiInit<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(
                ::ethers::contract::Contract::new(
                    address.into(),
                    DIAMONDMULTIINIT_ABI.clone(),
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
                DIAMONDMULTIINIT_ABI.clone(),
                DIAMONDMULTIINIT_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `multiInit` (0x6e02fa3c) function
        pub fn multi_init(
            &self,
            addresses: ::std::vec::Vec<::ethers::core::types::Address>,
            calldata: ::std::vec::Vec<::ethers::core::types::Bytes>,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([110, 2, 250, 60], (addresses, calldata))
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
    for DiamondMultiInit<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `AddressAndCalldataLengthDoNotMatch` with signature `AddressAndCalldataLengthDoNotMatch(uint256,uint256)` and selector `0x7340e166`
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
        name = "AddressAndCalldataLengthDoNotMatch",
        abi = "AddressAndCalldataLengthDoNotMatch(uint256,uint256)"
    )]
    pub struct AddressAndCalldataLengthDoNotMatch {
        pub addresses_length: ::ethers::core::types::U256,
        pub calldata_length: ::ethers::core::types::U256,
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
        Hash
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
        Hash
    )]
    #[etherror(
        name = "NoBytecodeAtAddress",
        abi = "NoBytecodeAtAddress(address,string)"
    )]
    pub struct NoBytecodeAtAddress {
        pub contract_address: ::ethers::core::types::Address,
        pub message: ::std::string::String,
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
    pub enum DiamondMultiInitErrors {
        AddressAndCalldataLengthDoNotMatch(AddressAndCalldataLengthDoNotMatch),
        InitializationFunctionReverted(InitializationFunctionReverted),
        NoBytecodeAtAddress(NoBytecodeAtAddress),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for DiamondMultiInitErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) = <AddressAndCalldataLengthDoNotMatch as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::AddressAndCalldataLengthDoNotMatch(decoded));
            }
            if let Ok(decoded) = <InitializationFunctionReverted as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::InitializationFunctionReverted(decoded));
            }
            if let Ok(decoded) = <NoBytecodeAtAddress as ::ethers::core::abi::AbiDecode>::decode(
                data,
            ) {
                return Ok(Self::NoBytecodeAtAddress(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for DiamondMultiInitErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::AddressAndCalldataLengthDoNotMatch(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::InitializationFunctionReverted(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NoBytecodeAtAddress(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for DiamondMultiInitErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector
                    == <AddressAndCalldataLengthDoNotMatch as ::ethers::contract::EthError>::selector() => {
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
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for DiamondMultiInitErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AddressAndCalldataLengthDoNotMatch(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::InitializationFunctionReverted(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::NoBytecodeAtAddress(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for DiamondMultiInitErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<AddressAndCalldataLengthDoNotMatch>
    for DiamondMultiInitErrors {
        fn from(value: AddressAndCalldataLengthDoNotMatch) -> Self {
            Self::AddressAndCalldataLengthDoNotMatch(value)
        }
    }
    impl ::core::convert::From<InitializationFunctionReverted>
    for DiamondMultiInitErrors {
        fn from(value: InitializationFunctionReverted) -> Self {
            Self::InitializationFunctionReverted(value)
        }
    }
    impl ::core::convert::From<NoBytecodeAtAddress> for DiamondMultiInitErrors {
        fn from(value: NoBytecodeAtAddress) -> Self {
            Self::NoBytecodeAtAddress(value)
        }
    }
    ///Container type for all input parameters for the `multiInit` function with signature `multiInit(address[],bytes[])` and selector `0x6e02fa3c`
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
    #[ethcall(name = "multiInit", abi = "multiInit(address[],bytes[])")]
    pub struct MultiInitCall {
        pub addresses: ::std::vec::Vec<::ethers::core::types::Address>,
        pub calldata: ::std::vec::Vec<::ethers::core::types::Bytes>,
    }
}
