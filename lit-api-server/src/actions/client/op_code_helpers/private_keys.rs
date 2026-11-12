use crate::utils::parse_with_hash::wallet_string_to_h160;
use crate::{core::get_verified_client_key, dstack::v1::get_lit_action_key};
use alloy::signers::local::PrivateKeySigner;
use anyhow::Result;
use lit_core::utils::binary::bytes_to_0x_hex;
use tracing::instrument;

/// Returned when an action asks for the private key of the account's admin or
/// billing wallet.
const ADMIN_WALLET_EXPORT_REFUSED: &str = "getPrivateKey cannot export the account's \
     admin wallet or its billing wallet. For an API-mode account this is the Account Master \
     Wallet, whose private key is the account's master API key. Use a separately created wallet \
     in Lit Actions.";

#[instrument(skip_all, err)]
pub async fn get_private_key(api_key: &str, pkp_id: &str) -> Result<String> {
    refuse_account_admin_wallet(api_key, pkp_id).await?;
    // get_verified_client_key resolves the path AND verifies the derived key's
    // address matches pkp_id, so an aliased (pkpId, path) registration can't
    // exfiltrate another wallet's key.
    let secret_bytes = get_verified_client_key(api_key, pkp_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get client key for private key: {e}"))?;
    let secret = bytes_to_0x_hex(secret_bytes);
    Ok(secret)
}

/// Refuse export of the account's cached admin or billing wallet.
///
/// `POST /new_account` base64-encodes the new account wallet's secret as the
/// master API key, makes that wallet the account's admin and billing wallet,
/// and registers it like any other wallet ("Account Master Wallet"). A group
/// can therefore grant it to a usage key, by address or through the
/// all-wallets entry, and actions only ever run under usage keys (an account
/// key has no execute grant). Releasing it would give a scoped usage key the
/// master API key and the contract's `msg.sender` admin rights over the
/// account.
///
/// The billing wallet is checked as well as the admin. Converting to
/// ChainSecured moves the admin to the user's wallet but keeps the Account
/// Master Wallet as the billing wallet, and `BillingOwnerAuth` still accepts
/// the old master API key because it controls that wallet: exporting it would
/// hand a usage key billing ownership (funding, saved-card auto top-up).
///
/// The check runs on the requested wallet before any key is derived. Actions
/// sign with a PKP only through this export, so it covers signing too; AES
/// encrypt/decrypt never reveal the key and are left alone. A ChainSecured
/// admin is normally the user's own wallet, which has no derivation here, so
/// only the error changes; if the user made one of the account's PKPs the
/// admin or billing wallet, that PKP is refused too, as its key carries the
/// same rights. Role changes take effect after mutation/event invalidation or
/// the fixed cache TTL; this guard accepts that propagation delay. A failed
/// cache fill refuses the export. Key/address verification remains uncached.
async fn refuse_account_admin_wallet(api_key: &str, pkp_id: &str) -> Result<()> {
    let requested = wallet_string_to_h160(pkp_id)
        .map_err(|e| anyhow::anyhow!("Unable to parse pkpId: {e:?}"))?;
    let fetch = async {
        let (admin, billing) = tokio::join!(
            crate::accounts::get_account_wallet_address(api_key),
            crate::accounts::get_billing_wallet_address(api_key),
        );
        let admin = admin
            .map_err(|e| anyhow::anyhow!("Unable to resolve the account's admin wallet: {e}"))?;
        let billing = billing
            .map_err(|e| anyhow::anyhow!("Unable to resolve the account's billing wallet: {e}"))?;
        let admin = wallet_string_to_h160(&admin)
            .map_err(|e| anyhow::anyhow!("Unable to parse the account's admin wallet: {e:?}"))?;
        let billing = wallet_string_to_h160(&billing)
            .map_err(|e| anyhow::anyhow!("Unable to parse the account's billing wallet: {e:?}"))?;
        Ok((admin, billing))
    };
    let (admin, billing) = if let Some(cache) = crate::accounts::blockchain_cache::get() {
        cache
            .protected_wallets(
                crate::utils::parse_with_hash::usage_api_key_to_hash(api_key),
                fetch,
            )
            .await?
    } else {
        fetch.await?
    };
    for wallet in [admin, billing] {
        if requested == wallet {
            anyhow::bail!(ADMIN_WALLET_EXPORT_REFUSED);
        }
    }
    Ok(())
}

#[instrument(skip_all, err)]
pub async fn get_lit_action_private_key(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action key for private key: {e}"))?;
    let secret = bytes_to_0x_hex(secret_bytes);
    Ok(secret)
}

#[instrument(skip_all, err)]
pub async fn get_lit_action_public_key(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action public key: {e}"))?;
    let signer = PrivateKeySigner::from_slice(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Unable to convert secret bytes to PrivateKeySigner: {e}"))?;
    let public_key_bytes = signer.credential().verifying_key().to_sec1_bytes();
    let public_key = bytes_to_0x_hex(&public_key_bytes);
    Ok(public_key)
}

#[instrument(skip_all, err)]
pub async fn get_lit_action_wallet_address(ipfs_id: &str) -> Result<String> {
    let secret_bytes = get_lit_action_key(ipfs_id)
        .await
        .map_err(|e| anyhow::anyhow!("Unable to get lit action wallet address: {e}"))?;
    let signer = PrivateKeySigner::from_slice(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Unable to convert secret bytes to PrivateKeySigner: {e}"))?;
    let wallet_address = signer.address();
    Ok(bytes_to_0x_hex(wallet_address.as_slice()))
}
