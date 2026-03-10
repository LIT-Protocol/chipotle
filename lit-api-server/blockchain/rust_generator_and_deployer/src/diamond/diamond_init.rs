pub use diamond_init::*;
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
pub mod diamond_init {
    const _: () = {
        ::core::include_bytes!(
            "./DiamondInit.json",
        );
    };
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("init"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("init"),
                            inputs: ::std::vec![],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::std::collections::BTreeMap::new(),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static DIAMONDINIT_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> = ::ethers::contract::Lazy::new(
        __abi,
    );
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80\x80`@R4`\x15Wa\x01L\x90\x81a\0\x1B\x829\xF3[`\0\x80\xFD\xFE`\x046\x10\x15a\0\rW`\0\x80\xFD[`\x005`\xE0\x1Cc\xE1\xC79*\x14a\0\"W`\0\x80\xFD[4a\x01\x11W`\x006`\x03\x19\x01\x12a\x01\x11W\x7F\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1E` R\x7Fg:&\xAB\x9C\x97m\xB9P\xBB\xE9\x87\xAA\x80\xC5\xE3\x87\xF3)V;\xB0\xAF\xE0\x93\xDD\xCC\xC9pH\x9E1\x80T`\x01`\xFF\x19\x91\x82\x16\x81\x17\x90\x92U\x7F\x9B\xED&S2\xEF\xC3\x0F\xA7d<\xC39\xED\xC9\x1C\xB2\x84\xA0\xF6Vh\x18\xA5x\x89\"\xAFX\xC8kP\x80T\x82\x16\x83\x17\x90U\x7Fy]\xB1X\x02\xE1Q\xB1\x92r\xD3\xE7\xB7.\xBD\x9D\x0C\xED\xC2\x82\xCC#\xA6\xE97\xC8\xC3\xC9\r\x9E!7\x80T\x82\x16\x83\x17\x90Uc\x07\xF5\x82\x8D`\xE4\x1B`\0R\x7F\xE6\x16\xBE\xA4fNYS(\xE5%\xB2I\x98!\x9C\xAE\xCE\xA2\t\r\xE9\x18GG:\xCF\xB3\xEF\xAA\x8A\xAD\x80T\x90\x91\x16\x90\x91\x17\x90U\0[`\0\x80\xFD\xFE\xA2dipfsX\"\x12 m\xFE9\x9BYo\xFA[\xCF9\xD8\xC3\xE8\xB3y\x12nR\xDB\xF8(\x1FU0\xBC\x98\xBB\xC1C8\xE5)dsolcC\0\x08\x1C\x003";
    /// The bytecode of the contract.
    pub static DIAMONDINIT_BYTECODE: ::ethers::core::types::Bytes = ::ethers::core::types::Bytes::from_static(
        __BYTECODE,
    );
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x046\x10\x15a\0\rW`\0\x80\xFD[`\x005`\xE0\x1Cc\xE1\xC79*\x14a\0\"W`\0\x80\xFD[4a\x01\x11W`\x006`\x03\x19\x01\x12a\x01\x11W\x7F\xC8\xFC\xAD\x8D\xB8M<\xC1\x8BLA\xD5Q\xEA\x0E\xE6m\xD5\x99\xCD\xE0h\xD9\x98\xE5}^\t3,\x13\x1E` R\x7Fg:&\xAB\x9C\x97m\xB9P\xBB\xE9\x87\xAA\x80\xC5\xE3\x87\xF3)V;\xB0\xAF\xE0\x93\xDD\xCC\xC9pH\x9E1\x80T`\x01`\xFF\x19\x91\x82\x16\x81\x17\x90\x92U\x7F\x9B\xED&S2\xEF\xC3\x0F\xA7d<\xC39\xED\xC9\x1C\xB2\x84\xA0\xF6Vh\x18\xA5x\x89\"\xAFX\xC8kP\x80T\x82\x16\x83\x17\x90U\x7Fy]\xB1X\x02\xE1Q\xB1\x92r\xD3\xE7\xB7.\xBD\x9D\x0C\xED\xC2\x82\xCC#\xA6\xE97\xC8\xC3\xC9\r\x9E!7\x80T\x82\x16\x83\x17\x90Uc\x07\xF5\x82\x8D`\xE4\x1B`\0R\x7F\xE6\x16\xBE\xA4fNYS(\xE5%\xB2I\x98!\x9C\xAE\xCE\xA2\t\r\xE9\x18GG:\xCF\xB3\xEF\xAA\x8A\xAD\x80T\x90\x91\x16\x90\x91\x17\x90U\0[`\0\x80\xFD\xFE\xA2dipfsX\"\x12 m\xFE9\x9BYo\xFA[\xCF9\xD8\xC3\xE8\xB3y\x12nR\xDB\xF8(\x1FU0\xBC\x98\xBB\xC1C8\xE5)dsolcC\0\x08\x1C\x003";
    /// The deployed bytecode of the contract.
    pub static DIAMONDINIT_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes = ::ethers::core::types::Bytes::from_static(
        __DEPLOYED_BYTECODE,
    );
    pub struct DiamondInit<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for DiamondInit<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for DiamondInit<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for DiamondInit<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for DiamondInit<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(DiamondInit))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> DiamondInit<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(
                ::ethers::contract::Contract::new(
                    address.into(),
                    DIAMONDINIT_ABI.clone(),
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
                DIAMONDINIT_ABI.clone(),
                DIAMONDINIT_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `init` (0xe1c7392a) function
        pub fn init(&self) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([225, 199, 57, 42], ())
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
    for DiamondInit<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Container type for all input parameters for the `init` function with signature `init()` and selector `0xe1c7392a`
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
    #[ethcall(name = "init", abi = "init()")]
    pub struct InitCall;
}
