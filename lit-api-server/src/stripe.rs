//! Stripe integration: create customer, charge via PaymentIntent, record meter events for usage.
//! See: https://docs.stripe.com/api (Customers, Payment Intents, Billing Meter Events).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

const STRIPE_API_BASE: &str = "https://api.stripe.com/v1";

fn stripe_secret_key() -> Option<String> {
    std::env::var("STRIPE_SECRET_KEY").ok().filter(|s| !s.is_empty())
}

/// Publishable key for the dashboard (Stripe.js). Not used server-side.
pub fn stripe_publishable_key() -> Option<String> {
    std::env::var("STRIPE_PUBLISHABLE_KEY").ok().filter(|s| !s.is_empty())
}

fn stripe_meter_event_name() -> String {
    std::env::var("STRIPE_METER_EVENT_NAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "mutable_action".to_string())
}

pub fn stripe_enabled() -> bool {
    stripe_secret_key().is_some()
}

/// Create a Stripe Customer and charge the given payment method (one-time).
/// Returns the Stripe customer id (e.g. cus_xxx) on success.
pub async fn create_customer_and_charge(
    client: &reqwest::Client,
    name: &str,
    payment_method_id: &str,
    amount_cents: u64,
) -> Result<String> {
    let key = stripe_secret_key().context("STRIPE_SECRET_KEY not set")?;

    // 1. Create customer
    let mut params = HashMap::new();
    params.insert("name".to_string(), name.to_string());
    params.insert("metadata[source]".to_string(), "lit_express_new_account".to_string());
    let customer: StripeCustomer = stripe_post(client, &key, "customers", params).await?;

    // 2. Attach payment method to customer
    let mut attach_params = HashMap::new();
    attach_params.insert("customer".to_string(), customer.id.clone());
    let _: serde_json::Value = stripe_post(
        client,
        &key,
        &format!("payment_methods/{}/attach", payment_method_id),
        attach_params,
    )
    .await?;

    // 3. Create and confirm PaymentIntent (charge)
    let mut pi_params = HashMap::new();
    pi_params.insert("amount".to_string(), amount_cents.to_string());
    pi_params.insert("currency".to_string(), "usd".to_string());
    pi_params.insert("customer".to_string(), customer.id.clone());
    pi_params.insert("payment_method".to_string(), payment_method_id.to_string());
    pi_params.insert("confirm".to_string(), "true".to_string());
    pi_params.insert("automatic_payment_methods[enabled]".to_string(), "false".to_string());
    pi_params.insert("metadata[source]".to_string(), "lit_express_initial_charge".to_string());
    let _: StripePaymentIntent = stripe_post(client, &key, "payment_intents", pi_params).await?;

    Ok(customer.id)
}

/// Record one meter event (1 unit = 1 cent) for the customer. Used after each mutable API action.
pub async fn record_meter_event(
    client: &reqwest::Client,
    stripe_customer_id: &str,
) -> Result<()> {
    let key = stripe_secret_key().context("STRIPE_SECRET_KEY not set")?;
    let event_name = stripe_meter_event_name();

    let mut params = HashMap::new();
    params.insert("event_name".to_string(), event_name);
    params.insert("payload[stripe_customer_id]".to_string(), stripe_customer_id.to_string());
    params.insert("payload[value]".to_string(), "1".to_string());
    let _: serde_json::Value = stripe_post(
        client,
        &key,
        "billing/meter_events",
        params,
    )
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct StripeCustomer {
    id: String,
}

#[derive(Debug, Deserialize)]
struct StripePaymentIntent {
    id: String,
    status: String,
}

/// POST form-encoded body to Stripe API. Response parsed as JSON.
async fn stripe_post<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    secret_key: &str,
    path: &str,
    params: HashMap<String, String>,
) -> Result<T> {
    let url = format!("{}/{}", STRIPE_API_BASE, path.trim_start_matches('/'));

    let res = client
        .post(&url)
        .bearer_auth(secret_key)
        .form(&params)
        .send()
        .await
        .context("Stripe request failed")?;

    let status = res.status();
    let text = res.text().await.context("Stripe response body")?;
    if !status.is_success() {
        anyhow::bail!("Stripe API error {}: {}", status, text);
    }
    serde_json::from_str(&text).context("Stripe response parse")
}
