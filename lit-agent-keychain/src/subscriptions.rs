use crate::{
    api::{self, ApiError, ApiResult},
    auth::{SameOrigin, Session},
    billing,
    config::Config,
    crypto,
    stripe::{self, Stripe},
};
use rocket::{
    data::ToByteUnit,
    get,
    http::Status,
    post,
    request::{FromRequest, Outcome},
    serde::json::Json,
    Data, Request, State,
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;

// Every vault starts on Free: a handful of secrets with sponsored execution so
// the product can be tried without a card. Paying raises the storage limit.
pub const FREE_LIMIT: i64 = 5;
pub const STANDARD_LIMIT: i64 = 1000;
pub const MAX_CUSTOM_LIMIT: i64 = 100000;
#[derive(sqlx::FromRow)]
pub struct Subscription {
    pub customer_id: Option<String>,
    pub subscription_id: Option<String>,
    pub status: String,
    pub paid_until: Option<OffsetDateTime>,
    pub cancel_at_period_end: bool,
    pub checkout_id: Option<String>,
    pub checkout_generation: i64,
    pub custom_secret_limit: Option<i64>,
    pub custom_until: Option<OffsetDateTime>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan: &'static str,
    pub active: bool,
    pub secret_limit: i64,
    pub paid_until: Option<i64>,
    pub cancel_at_period_end: bool,
    pub status: String,
}
impl Subscription {
    pub fn plan(&self, now: OffsetDateTime) -> Plan {
        let custom =
            self.custom_secret_limit.is_some() && self.custom_until.is_some_and(|end| end > now);
        let active =
            custom || (self.status == "active" && self.paid_until.is_some_and(|end| end > now));
        Plan {
            plan: if custom {
                "custom"
            } else if active {
                "standard"
            } else {
                "free"
            },
            active,
            secret_limit: if custom {
                self.custom_secret_limit.unwrap_or(STANDARD_LIMIT)
            } else if active {
                STANDARD_LIMIT
            } else {
                FREE_LIMIT
            },
            paid_until: if custom {
                self.custom_until
            } else {
                self.paid_until
            }
            .map(|t| t.unix_timestamp()),
            cancel_at_period_end: !custom && self.cancel_at_period_end,
            status: self.status.clone(),
        }
    }
}
pub async fn lock(
    tx: &mut Transaction<'_, Postgres>,
    vault: &str,
) -> Result<Subscription, ApiError> {
    crate::registry::lock_vault(tx, vault).await?;
    sqlx::query("INSERT INTO kc_subscriptions(vault_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(vault)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
    sqlx::query_as("SELECT * FROM kc_subscriptions WHERE vault_id=$1 FOR UPDATE")
        .bind(vault)
        .fetch_one(&mut **tx)
        .await
        .map_err(api::internal)
}
// Storage writes must fit the current plan. Creating needs a free slot; other
// writes (rotation, restore) only require the vault not to exceed its limit,
// so a lapsed subscriber keeps working until they are over the Free limit.
// Over the Free limit is a payment problem (402); at a paid limit it is
// capacity (409). Callers hold the vault lock, so counts are race-free.
pub async fn require_capacity(
    tx: &mut Transaction<'_, Postgres>,
    vault: &str,
    creating: bool,
) -> Result<Plan, ApiError> {
    let plan = lock(tx, vault).await?.plan(OffsetDateTime::now_utc());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM kc_secrets WHERE vault_id=$1")
        .bind(vault)
        .fetch_one(&mut **tx)
        .await
        .map_err(api::internal)?;
    if count + i64::from(creating) > plan.secret_limit {
        return Err(if plan.active {
            api::err(Status::Conflict, "secret_limit")
        } else {
            api::err(Status::PaymentRequired, "subscription_required")
        });
    }
    Ok(plan)
}
fn unavailable(_: impl std::fmt::Display) -> ApiError {
    api::err(Status::BadGateway, "billing_unavailable")
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    v[key]
        .as_str()
        .ok_or_else(|| unavailable("invalid Stripe response"))
}

// Read current Stripe state while holding the vault lock. Event payloads and
// delivery order never select access; retries and old events re-read the provider.
pub async fn sync_locked(
    tx: &mut Transaction<'_, Postgres>,
    stripe: &Stripe,
    vault: &str,
    subscription: &Subscription,
) -> Result<(), ApiError> {
    let Some(customer) = &subscription.customer_id else {
        return Ok(());
    };
    let mut subscriptions = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..10 {
        let mut query = vec![
            ("customer", customer.clone()),
            ("status", "all".into()),
            ("limit", "100".into()),
            ("expand[]", "data.latest_invoice".into()),
        ];
        if let Some(after) = &after {
            query.push(("starting_after", after.clone()));
        }
        let page = stripe
            .get("/subscriptions", &query)
            .await
            .map_err(unavailable)?;
        let data = page["data"]
            .as_array()
            .ok_or_else(|| unavailable("invalid subscriptions"))?;
        subscriptions.extend(data.iter().cloned());
        if page["has_more"] == false {
            after = None;
            break;
        }
        after =
            Some(string(data.last().ok_or_else(|| unavailable("empty page"))?, "id")?.to_owned());
    }
    if after.is_some() {
        return Err(unavailable("too many subscriptions"));
    }
    let mut matching: Vec<_> = subscriptions
        .iter()
        .filter(|s| {
            s["customer"] == *customer
                && s["metadata"]["keychain_vault_id"] == vault
                && s["metadata"]["app"] == "lit-agent-keychain"
                && s["items"]["data"].as_array().is_some_and(|items| {
                    items.len() == 1
                        && items[0]["price"]["id"] == stripe.price
                        && items[0]["quantity"] == 1
                })
        })
        .collect();
    matching.sort_by_key(|s| (s["status"] == "active", s["created"].as_i64().unwrap_or(0)));
    let latest = matching.last();
    let (id, status, until, cancel) = if let Some(s) = latest {
        let paid = s["latest_invoice"]["status"] == "paid";
        let end = if paid {
            s["items"]["data"][0]["current_period_end"]
                .as_i64()
                .and_then(|t| OffsetDateTime::from_unix_timestamp(t).ok())
        } else {
            None
        };
        (
            Some(string(s, "id")?.to_owned()),
            string(s, "status")?.to_owned(),
            end,
            s["cancel_at_period_end"] == true,
        )
    } else {
        (None, "none".into(), None, false)
    };
    if id != subscription.subscription_id
        || status != subscription.status
        || until != subscription.paid_until
        || cancel != subscription.cancel_at_period_end
    {
        sqlx::query("INSERT INTO kc_audit(vault_id,event) VALUES($1,'subscription_updated')")
            .bind(vault)
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
    }
    sqlx::query("UPDATE kc_subscriptions SET subscription_id=$2,status=$3,paid_until=$4,cancel_at_period_end=$5,synced_at=now(),reconcile_after=now()+interval '5 minutes' WHERE vault_id=$1")
        .bind(vault).bind(id).bind(status).bind(until).bind(cancel).execute(&mut **tx).await.map_err(api::internal)?;
    Ok(())
}
pub async fn refresh(pool: &PgPool, stripe: &Stripe, vault: &str) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let sub = lock(&mut tx, vault).await?;
    sync_locked(&mut tx, stripe, vault, &sub).await?;
    tx.commit().await.map_err(api::internal)
}
#[get("/api/billing")]
pub async fn status(
    session: Session,
    pool: &State<PgPool>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let sub = lock(&mut tx, &session.vault_id).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM kc_secrets WHERE vault_id=$1")
        .bind(&session.vault_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(api::internal)?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(
        json!({"subscription":sub.plan(OffsetDateTime::now_utc()),"secretCount":count,"priceCents":1000,"currency":"usd","contactEmail":cfg.contact_email,"canManageBilling":sub.customer_id.is_some()}),
    ))
}
#[post("/api/billing/refresh")]
pub async fn refresh_route(
    _origin: SameOrigin,
    session: Session,
    pool: &State<PgPool>,
    stripe: &State<Stripe>,
) -> ApiResult<Value> {
    billing::reserve(
        pool,
        &format!("billing-refresh:{}", session.vault_id),
        3600,
        60,
    )
    .await?;
    refresh(pool, stripe, &session.vault_id).await?;
    Ok(Json(json!({"ok":true})))
}
#[post("/api/billing/checkout")]
pub async fn checkout(
    _origin: SameOrigin,
    session: Session,
    pool: &State<PgPool>,
    stripe: &State<Stripe>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    billing::reserve(pool, &format!("checkout:{}", session.vault_id), 3600, 20).await?;
    let vault = &session.vault_id;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let mut sub = lock(&mut tx, vault).await?;
    let customer = if let Some(customer) = sub.customer_id.clone() {
        customer
    } else {
        let result = stripe
            .post(
                "/customers",
                &[
                    ("metadata[keychain_vault_id]", vault.clone()),
                    ("metadata[app]", "lit-agent-keychain".into()),
                ],
                &format!("kc-customer-{vault}"),
            )
            .await
            .map_err(unavailable)?;
        let customer = string(&result, "id")?.to_owned();
        if !stripe::valid_id(&customer, "cus_") {
            return Err(unavailable("invalid customer"));
        }
        sqlx::query("UPDATE kc_subscriptions SET customer_id=$2 WHERE vault_id=$1")
            .bind(vault)
            .bind(&customer)
            .execute(&mut *tx)
            .await
            .map_err(api::internal)?;
        customer
    };
    // Persist the customer before creating any subscription; webhook routing is stable.
    tx.commit().await.map_err(api::internal)?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    sub = lock(&mut tx, vault).await?;
    sync_locked(&mut tx, stripe, vault, &sub).await?;
    sub = lock(&mut tx, vault).await?;
    if sub.plan(OffsetDateTime::now_utc()).active
        || matches!(
            sub.status.as_str(),
            "active" | "past_due" | "unpaid" | "incomplete" | "trialing" | "paused"
        )
    {
        tx.commit().await.map_err(api::internal)?;
        return Err(api::err(Status::Conflict, "manage_existing_subscription"));
    }
    if let Some(id) = &sub.checkout_id {
        let existing = stripe
            .get(&format!("/checkout/sessions/{id}"), &[])
            .await
            .map_err(unavailable)?;
        if existing["status"] == "open" {
            let url = stripe_url(&existing, "checkout.stripe.com", cfg)?;
            tx.commit().await.map_err(api::internal)?;
            return Ok(Json(json!({"url":url})));
        }
        // A completed session may be awaiting payment/webhook delivery. Never
        // create a second subscription while Stripe says its checkout completed.
        if existing["status"] == "complete" && sub.subscription_id.is_none() {
            tx.commit().await.map_err(api::internal)?;
            return Err(api::err(Status::Conflict, "payment_pending"));
        }
    }
    let generation = sub
        .checkout_generation
        .checked_add(1)
        .ok_or_else(|| unavailable("generation overflow"))?;
    let result = stripe
        .post(
            "/checkout/sessions",
            &[
                ("mode", "subscription".into()),
                ("customer", customer),
                ("line_items[0][price]", stripe.price.clone()),
                ("line_items[0][quantity]", "1".into()),
                ("payment_method_types[0]", "card".into()),
                ("client_reference_id", vault.clone()),
                (
                    "subscription_data[metadata][keychain_vault_id]",
                    vault.clone(),
                ),
                (
                    "subscription_data[metadata][app]",
                    "lit-agent-keychain".into(),
                ),
                (
                    "success_url",
                    format!("{}/?checkout=success", cfg.public_base_url),
                ),
                (
                    "cancel_url",
                    format!("{}/?checkout=canceled", cfg.public_base_url),
                ),
            ],
            &format!("kc-checkout-{vault}-{generation}"),
        )
        .await
        .map_err(unavailable)?;
    let id = string(&result, "id")?;
    if !stripe::valid_id(id, "cs_") {
        return Err(unavailable("invalid checkout"));
    }
    let url = stripe_url(&result, "checkout.stripe.com", cfg)?;
    sqlx::query(
        "UPDATE kc_subscriptions SET checkout_id=$2,checkout_generation=$3 WHERE vault_id=$1",
    )
    .bind(vault)
    .bind(id)
    .bind(generation)
    .execute(&mut *tx)
    .await
    .map_err(api::internal)?;
    sqlx::query("INSERT INTO kc_audit(vault_id,event) VALUES($1,'checkout_created')")
        .bind(vault)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"url":url})))
}
fn stripe_url(result: &Value, host: &str, cfg: &Config) -> Result<String, ApiError> {
    let url = string(result, "url")?;
    let parsed = reqwest::Url::parse(url).map_err(unavailable)?;
    if (parsed.scheme() != "https"
        || parsed.host_str() != Some(host)
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some())
        && (cfg.stripe_api_url == "https://api.stripe.com"
            || parsed.origin().ascii_serialization() != cfg.stripe_api_url)
    {
        return Err(unavailable("invalid redirect"));
    }
    Ok(url.into())
}
#[post("/api/billing/portal")]
pub async fn portal(
    _origin: SameOrigin,
    session: Session,
    pool: &State<PgPool>,
    stripe: &State<Stripe>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    billing::reserve(pool, &format!("portal:{}", session.vault_id), 3600, 30).await?;
    let customer: Option<String> =
        sqlx::query_scalar("SELECT customer_id FROM kc_subscriptions WHERE vault_id=$1")
            .bind(&session.vault_id)
            .fetch_optional(pool.inner())
            .await
            .map_err(api::internal)?
            .flatten();
    let customer = customer.ok_or_else(|| api::err(Status::NotFound, "no_billing_account"))?;
    let result = stripe
        .post(
            "/billing_portal/sessions",
            &[
                ("customer", customer),
                ("configuration", stripe.portal_configuration.clone()),
                ("return_url", format!("{}/", cfg.public_base_url)),
            ],
            &format!("kc-portal-{}", crypto::random_token()),
        )
        .await
        .map_err(unavailable)?;
    Ok(Json(
        json!({"url":stripe_url(&result,"billing.stripe.com",cfg)?}),
    ))
}
pub struct StripeSignature(String);
#[rocket::async_trait]
impl<'r> FromRequest<'r> for StripeSignature {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        match req.headers().get_one("Stripe-Signature") {
            Some(value) => Outcome::Success(Self(value.into())),
            None => Outcome::Error((Status::BadRequest, ())),
        }
    }
}
#[post("/api/billing/webhook", data = "<data>")]
pub async fn webhook(
    signature: StripeSignature,
    data: Data<'_>,
    pool: &State<PgPool>,
    stripe: &State<Stripe>,
) -> Result<Status, ApiError> {
    let bytes = data
        .open(256.kibibytes())
        .into_bytes()
        .await
        .map_err(|_| api::err(Status::BadRequest, "invalid_webhook"))?;
    if !bytes.is_complete() {
        return Err(api::err(Status::PayloadTooLarge, "webhook_too_large"));
    }
    let event = stripe
        .verify_webhook(
            &signature.0,
            &bytes,
            OffsetDateTime::now_utc().unix_timestamp(),
        )
        .map_err(|_| api::err(Status::BadRequest, "invalid_webhook"))?;
    let id = string(&event, "id")?;
    if !stripe::valid_id(id, "evt_") {
        return Err(api::err(Status::BadRequest, "invalid_webhook"));
    }
    let kind = string(&event, "type")?;
    if !matches!(
        kind,
        "checkout.session.completed"
            | "checkout.session.async_payment_succeeded"
            | "checkout.session.async_payment_failed"
            | "customer.subscription.created"
            | "customer.subscription.updated"
            | "customer.subscription.deleted"
            | "invoice.paid"
            | "invoice.payment_failed"
    ) {
        return Ok(Status::Ok);
    }
    let Some(customer) = event["data"]["object"]["customer"].as_str() else {
        return Err(api::err(Status::BadRequest, "invalid_webhook"));
    };
    let vault: Option<String> =
        sqlx::query_scalar("SELECT vault_id FROM kc_subscriptions WHERE customer_id=$1")
            .bind(customer)
            .fetch_optional(pool.inner())
            .await
            .map_err(api::internal)?;
    let Some(vault) = vault else {
        return Ok(Status::Ok);
    };
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let sub = lock(&mut tx, &vault).await?;
    let inserted =
        sqlx::query("INSERT INTO kc_stripe_events(id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(api::internal)?;
    if inserted.rows_affected() > 0 {
        sync_locked(&mut tx, stripe, &vault, &sub).await?;
    }
    tx.commit().await.map_err(api::internal)?;
    Ok(Status::Ok)
}
