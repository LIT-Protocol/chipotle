//! Opt-in Stripe TEST MODE contract test. Temporary provider objects are cleaned
//! up even if validation fails; local DB changes are rolled back. Never use live keys.
use anyhow::{ensure, Context, Result};
use lit_agent_keychain::{
    config::Config,
    crypto, db,
    stripe::{Stripe, VERSION},
    subscriptions,
};
use serde_json::Value;
use time::OffsetDateTime;
fn id(v: &Value) -> Result<String> {
    Ok(v["id"].as_str().context("missing Stripe object ID")?.into())
}
#[tokio::test]
#[ignore = "requires Stripe test key and a migrated dedicated local database"]
async fn stripe_test_mode_subscription_lifecycle() -> Result<()> {
    let secret = std::env::var("STRIPE_SECRET_KEY")?;
    ensure!(
        secret.starts_with("sk_test_") || secret.starts_with("rk_test_"),
        "test-mode key required"
    );
    let database = std::env::var("KEYCHAIN_TEST_DATABASE_URL")?;
    let url = reqwest::Url::parse(&database)?;
    ensure!(
        matches!(url.host_str(), Some("localhost" | "127.0.0.1")) && url.path().contains("test"),
        "dedicated loopback test DB required"
    );
    let cfg = Config {
        database_url: database.clone(),
        public_base_url: "http://localhost:55441".into(),
        lit_api_url: "https://api.chipotle.litprotocol.com".into(),
        lit_execution_key: "unused".into(),
        chipotle_master_key: "unused".into(),
        usage_key_encryption_key: [0; 32],
        stripe_secret_key: secret.clone(),
        stripe_webhook_secret: "unused".into(),
        stripe_price_id: "price_unused".into(),
        stripe_portal_configuration: "bpc_unused".into(),
        stripe_api_url: "https://api.stripe.com".into(),
        contact_email: "unused@example.com".into(),
        network: "test".into(),
        google_client_id: None,
        daily_execution_limit: 1,
        hourly_ip_execution_limit: 1,
        daily_vault_execution_limit: 1,
        secure_cookies: false,
        web_dir: "unused".into(),
    };
    let mut stripe = Stripe::new(&cfg)?;
    let nonce = crypto::random_token();
    let pool = db::connect(&database).await?;
    let mut tx = pool.begin().await?;
    let vault = crypto::digest(&serde_json::json!({"test":nonce}))?;
    sqlx::query("INSERT INTO kc_vaults(id,authority,authority_cid) VALUES($1,'{}','fixture')")
        .bind(&vault)
        .execute(&mut *tx)
        .await?;
    let mut customer = None;
    let mut product = None;
    let mut price = None;
    let mut portal = None;
    let mut checkout = None;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let result: Result<()> = async {
        product = Some(id(&stripe
            .post(
                "/products",
                &[("name", "Keychain temporary contract test".into())],
                &format!("{nonce}-product"),
            )
            .await?)?);
        price = Some(id(&stripe
            .post(
                "/prices",
                &[
                    ("product", product.clone().unwrap()),
                    ("currency", "usd".into()),
                    ("unit_amount", "1000".into()),
                    ("recurring[interval]", "month".into()),
                ],
                &format!("{nonce}-price"),
            )
            .await?)?);
        portal = Some(id(&stripe
            .post(
                "/billing_portal/configurations",
                &[
                    ("metadata[keychain_contract_test]", nonce.clone()),
                    ("features[subscription_cancel][enabled]", "true".into()),
                    (
                        "features[subscription_cancel][mode]",
                        "at_period_end".into(),
                    ),
                    ("features[subscription_update][enabled]", "false".into()),
                    ("features[payment_method_update][enabled]", "true".into()),
                    ("features[invoice_history][enabled]", "true".into()),
                ],
                &format!("{nonce}-portal"),
            )
            .await?)?);
        stripe.price = price.clone().unwrap();
        stripe.portal_configuration = portal.clone().unwrap();
        stripe.validate_configuration().await?;
        customer = Some(id(&stripe
            .post(
                "/customers",
                &[
                    ("description", "Keychain temporary contract test".into()),
                    ("metadata[app]", "lit-agent-keychain".into()),
                    ("metadata[keychain_vault_id]", vault.clone()),
                ],
                &format!("{nonce}-customer"),
            )
            .await?)?);
        let customer_id = customer.clone().unwrap();
        subscriptions::lock(&mut tx, &vault)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sqlx::query("UPDATE kc_subscriptions SET customer_id=$2 WHERE vault_id=$1")
            .bind(&vault)
            .bind(&customer_id)
            .execute(&mut *tx)
            .await?;
        let session = stripe
            .post(
                "/checkout/sessions",
                &[
                    ("mode", "subscription".into()),
                    ("customer", customer_id.clone()),
                    ("line_items[0][price]", stripe.price.clone()),
                    ("line_items[0][quantity]", "1".into()),
                    ("payment_method_types[0]", "card".into()),
                    (
                        "success_url",
                        "http://localhost:55441/?checkout=success".into(),
                    ),
                    ("cancel_url", "http://localhost:55441/".into()),
                    (
                        "subscription_data[metadata][app]",
                        "lit-agent-keychain".into(),
                    ),
                    (
                        "subscription_data[metadata][keychain_vault_id]",
                        vault.clone(),
                    ),
                ],
                &format!("{nonce}-checkout"),
            )
            .await?;
        checkout = Some(id(&session)?);
        ensure!(
            session["url"]
                .as_str()
                .is_some_and(|u| u.starts_with("https://checkout.stripe.com/")),
            "invalid checkout URL"
        );
        // Documented Stripe test payment method; no real card data or payment.
        let payment_method = id(&stripe
            .post(
                "/payment_methods/pm_card_visa/attach",
                &[("customer", customer_id.clone())],
                &format!("{nonce}-card"),
            )
            .await?)?;
        let sub = stripe
            .post(
                "/subscriptions",
                &[
                    ("customer", customer_id.clone()),
                    ("items[0][price]", stripe.price.clone()),
                    ("items[0][quantity]", "1".into()),
                    ("default_payment_method", payment_method),
                    ("payment_behavior", "error_if_incomplete".into()),
                    ("metadata[app]", "lit-agent-keychain".into()),
                    ("metadata[keychain_vault_id]", vault.clone()),
                ],
                &format!("{nonce}-subscription"),
            )
            .await?;
        let sub_id = id(&sub)?;
        for canceled in [false, true] {
            if canceled {
                stripe
                    .post(
                        &format!("/subscriptions/{sub_id}"),
                        &[("cancel_at_period_end", "true".into())],
                        &format!("{nonce}-cancel"),
                    )
                    .await?;
            }
            let current = subscriptions::lock(&mut tx, &vault)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            subscriptions::sync_locked(&mut tx, &stripe, &vault, &current)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            let plan = subscriptions::lock(&mut tx, &vault)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?
                .plan(OffsetDateTime::now_utc());
            ensure!(
                plan.active && plan.secret_limit == 1000 && plan.cancel_at_period_end == canceled,
                "incorrect paid entitlement from real Stripe response"
            );
        }
        let response = http
            .delete(format!("https://api.stripe.com/v1/subscriptions/{sub_id}"))
            .bearer_auth(&secret)
            .header("Stripe-Version", VERSION)
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "test subscription cancellation failed"
        );
        let current = subscriptions::lock(&mut tx, &vault)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        subscriptions::sync_locked(&mut tx, &stripe, &vault, &current)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        ensure!(
            !subscriptions::lock(&mut tx, &vault)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?
                .plan(OffsetDateTime::now_utc())
                .active,
            "canceled Stripe subscription remained active"
        );
        Ok(())
    }
    .await;
    eprintln!("Stripe contract result: {result:?}");
    let mut cleanup_failed = false;
    if let Some(id) = checkout {
        let cleanup = stripe
            .post(
                &format!("/checkout/sessions/{id}/expire"),
                &[],
                &format!("{nonce}-expire"),
            )
            .await;
        if let Err(error) = cleanup {
            eprintln!("Checkout cleanup {id}: {error}");
            cleanup_failed = true;
        }
    }
    if let Some(id) = customer {
        let response = http
            .delete(format!("https://api.stripe.com/v1/customers/{id}"))
            .bearer_auth(&secret)
            .header("Stripe-Version", VERSION)
            .send()
            .await;
        if !matches!(&response,Ok(r) if r.status().is_success()) {
            eprintln!("Customer cleanup failed: {id}");
            cleanup_failed = true;
        }
    }
    for (kind, object) in [
        ("prices", price),
        ("products", product),
        ("billing_portal/configurations", portal),
    ] {
        if let Some(id) = object {
            let cleanup = stripe
                .post(
                    &format!("/{kind}/{id}"),
                    &[("active", "false".into())],
                    &format!("{nonce}-archive-{kind}"),
                )
                .await;
            if let Err(error) = cleanup {
                eprintln!("Cleanup {kind} {id}: {error}");
                cleanup_failed = true;
            }
        }
    }
    tx.rollback().await?;
    ensure!(
        !cleanup_failed,
        "Stripe test cleanup incomplete; inspect temporary contract-test objects"
    );
    result
}
