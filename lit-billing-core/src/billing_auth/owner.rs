//! Billing administration requires the master wallet, not execution permission.
//! Managed master keys encode their billing wallet's private key; usage keys
//! encode different wallets. Check against the independently resolved account.
use super::{AuthError, AuthResolver, BillingAuth};
use alloy_primitives::Address;
use base64::{Engine, engine::general_purpose::STANDARD};
use rocket::{
    http::Status,
    request::{FromRequest, Outcome, Request},
};
use std::{ops::Deref, sync::Arc};

pub struct BillingOwnerAuth(pub BillingAuth);
impl Deref for BillingOwnerAuth {
    type Target = BillingAuth;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
pub fn key_controls_wallet(raw: &str, wallet: &str) -> bool {
    let Ok(mut bytes) = STANDARD.decode(raw) else {
        return false;
    };
    let signer = k256::ecdsa::SigningKey::from_slice(&bytes);
    bytes.fill(0);
    let (Ok(signer), Ok(expected)) = (signer, wallet.parse::<Address>()) else {
        return false;
    };
    Address::from_private_key(&signer) == expected
}
#[rocket::async_trait]
impl<'r> FromRequest<'r> for BillingOwnerAuth {
    type Error = ();
    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, ()> {
        let auth = match request.guard::<BillingAuth>().await {
            Outcome::Success(auth) => auth,
            Outcome::Error(error) => return Outcome::Error(error),
            Outcome::Forward(status) => return Outcome::Forward(status),
        };
        if let BillingAuth::ApiKey(key) = &auth {
            let Some(resolver) = request.rocket().state::<Arc<dyn AuthResolver>>() else {
                return Outcome::Error((Status::ServiceUnavailable, ()));
            };
            match resolver.resolve_api_key(key).await {
                Ok(identity) if key_controls_wallet(key, &identity.wallet_address_hex) => {}
                Ok(_) => return Outcome::Error((Status::Forbidden, ())),
                Err(AuthError::Transient(_)) => {
                    return Outcome::Error((Status::ServiceUnavailable, ()));
                }
                Err(_) => return Outcome::Error((Status::Unauthorized, ())),
            }
        }
        Outcome::Success(Self(auth))
    }
}
#[cfg(feature = "openapi")]
impl<'r> rocket_okapi::request::OpenApiFromRequest<'r> for BillingOwnerAuth {
    fn from_request_input(
        generator: &mut rocket_okapi::r#gen::OpenApiGenerator,
        name: String,
        required: bool,
    ) -> rocket_okapi::Result<rocket_okapi::request::RequestHeaderInput> {
        use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
        let mut input =
            <BillingAuth as OpenApiFromRequest>::from_request_input(generator, name, required)?;
        if let RequestHeaderInput::Parameter(parameter) = &mut input {
            parameter.description=Some("Billing owner only: account master API key or verified X-Wallet-Auth. Execution usage keys cannot manage funding or saved-card settings.".into());
        }
        Ok(input)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_auth::{ResolvedIdentity, WalletAuthPayload};
    use rocket::{get, http::Header, local::asynchronous::Client, routes};
    struct Resolver {
        wallet: String,
    }
    #[async_trait::async_trait]
    impl AuthResolver for Resolver {
        async fn verify_wallet_auth(
            &self,
            _: &WalletAuthPayload,
        ) -> Result<ResolvedIdentity, AuthError> {
            Ok(ResolvedIdentity {
                wallet_address_hex: self.wallet.clone(),
                api_key_hash_hex: format!("0x{}", "11".repeat(32)),
            })
        }
        async fn resolve_api_key(&self, _: &str) -> Result<ResolvedIdentity, AuthError> {
            Ok(ResolvedIdentity {
                wallet_address_hex: self.wallet.clone(),
                api_key_hash_hex: format!("0x{}", "11".repeat(32)),
            })
        }
    }
    #[get("/owner")]
    fn owner(_auth: BillingOwnerAuth) -> &'static str {
        "ok"
    }
    #[get("/usage")]
    fn usage(_auth: BillingAuth) -> &'static str {
        "ok"
    }
    #[tokio::test]
    async fn resolved_usage_key_cannot_manage_parent_billing() {
        let master = STANDARD.encode([7u8; 32]);
        let usage_key = STANDARD.encode([8u8; 32]);
        let signer = k256::ecdsa::SigningKey::from_slice(&[7u8; 32]).unwrap();
        let wallet = Address::from_private_key(&signer).to_string();
        assert!(key_controls_wallet(&master, &wallet));
        assert!(!key_controls_wallet(&usage_key, &wallet));
        assert!(!key_controls_wallet("invalid", &wallet));
        let resolver: Arc<dyn AuthResolver> = Arc::new(Resolver { wallet });
        let client = Client::tracked(
            rocket::build()
                .manage(resolver)
                .mount("/", routes![owner, usage]),
        )
        .await
        .unwrap();
        assert_eq!(
            client
                .get("/usage")
                .header(Header::new("X-Api-Key", usage_key.clone()))
                .dispatch()
                .await
                .status(),
            Status::Ok
        );
        assert_eq!(
            client
                .get("/owner")
                .header(Header::new("X-Api-Key", usage_key))
                .dispatch()
                .await
                .status(),
            Status::Forbidden
        );
        assert_eq!(
            client
                .get("/owner")
                .header(Header::new("X-Api-Key", master))
                .dispatch()
                .await
                .status(),
            Status::Ok
        );
        let wallet_auth = STANDARD.encode(
            serde_json::to_vec(&serde_json::json!({"typed_data":{},"signature":"fixture"}))
                .unwrap(),
        );
        assert_eq!(
            client
                .get("/owner")
                .header(Header::new("X-Wallet-Auth", wallet_auth))
                .dispatch()
                .await
                .status(),
            Status::Ok
        );
    }
}
