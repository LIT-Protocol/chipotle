use derive_more::Display;
use lit_core::error::*;
pub use lit_core::error::{Error, Result, Unexpected};
use lit_core::generate_pkg_constructors;
use lit_core_derive::{Description, ErrorCode};
use std::fmt::Debug;
pub const PKG_NAME: &str = "lit_node";

// constructors

#[derive(Debug, Display, Description, ErrorCode)]
#[allow(dead_code)]
pub(crate) enum EC {
    /// A general system fault has occurred in the node
    #[code(kind = Unexpected, http_status = 500)]
    NodeSystemFault,
    /// Lit nodes failed to check the condition possibly due to RPC servers being down or because the condition is making an incorrect smart contract call that reverts
    #[code(kind = Validation, http_status = 502)]
    NodeAccessControlConditionsCheckFailed,
    /// The access control condition check returned that you are not permitted to access this content.  Are you sure you meet the conditions?  Check the auth_sig and the other conditions
    #[code(kind = Validation, http_status = 401)]
    NodeAccessControlConditionsReturnedNotAuthorized,
    /// Failed to find the passed encrypted symmetric key
    #[code(kind = Validation, http_status = 404)]
    NodeEncryptedSymmetricKeyNotFound,
    /// While signing JWT, EXP too large or wrong
    #[code(kind = Validation, http_status = 403)]
    NodeExpWrongOrTooLarge,
}

generate_pkg_constructors!(PKG_NAME, pub(crate), EC);
