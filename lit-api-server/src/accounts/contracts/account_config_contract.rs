#![allow(clippy::too_many_arguments)]

use alloy::sol;

sol!(
    #[sol(rpc)]
    AccountConfig,
    "src/accounts/contracts/AccountConfig.json"
);

pub use AccountConfig::AccountConfigErrors;
pub use AppStorage::{Metadata, PkpData};
pub use ViewsFacet::{KeyValueReturn, UsageApiKeyReturn};
