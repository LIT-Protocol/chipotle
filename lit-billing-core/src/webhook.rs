//! Stripe webhook signature verification + event interpretation (CPL-335).
//!
//! Why this exists: credits are granted on a client-initiated
//! `POST /billing/confirm_payment`, but a card chargeback, a dashboard refund,
//! or a Stripe-side reversal pulls the money back *without* ever touching the
//! internal credit ledger. Without a webhook the customer keeps the credits
//! they no longer paid for — unbounded, trivially-exploitable loss.
//!
//! This module holds the *pure* pieces so they're unit-testable without any
//! HTTP: signature verification, event parsing, and the
//! event → [`Clawback`] interpretation. The Stripe API calls (resolving the
//! customer for a dispute, writing the debiting balance transaction) and the
//! idempotency cache live in `lit-api-server`'s `stripe` module, mirroring the
//! existing split where `http.rs` is pure and the charge/PaymentIntent flows
//! live in the server.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Stripe's default replay tolerance: reject a signature whose timestamp is
/// more than 5 minutes from now. Matches the official Stripe libraries.
pub const DEFAULT_TOLERANCE_SECS: i64 = 300;

/// Why signature verification rejected a webhook delivery.
#[derive(Debug, PartialEq, Eq)]
pub enum SignatureError {
    /// `Stripe-Signature` header had no `t=` timestamp or no `v1=` signature.
    MalformedHeader,
    /// The timestamp is outside `tolerance_secs` of now — likely a replay.
    TimestampOutOfTolerance,
    /// No `v1` signature in the header matched the expected HMAC. Either the
    /// payload was tampered with or the wrong signing secret is configured.
    NoMatchingSignature,
}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignatureError::MalformedHeader => write!(f, "malformed Stripe-Signature header"),
            SignatureError::TimestampOutOfTolerance => {
                write!(f, "Stripe-Signature timestamp outside tolerance")
            }
            SignatureError::NoMatchingSignature => {
                write!(f, "no Stripe-Signature v1 entry matched the expected HMAC")
            }
        }
    }
}

impl std::error::Error for SignatureError {}

/// Verify a `Stripe-Signature` header against the raw request body.
///
/// Implements the scheme Stripe documents: the signed payload is
/// `"{timestamp}.{raw_body}"`, HMAC-SHA256'd with the endpoint's signing
/// secret (the `whsec_…` string, used verbatim as the key). The header carries
/// the timestamp (`t=`) and one or more candidate signatures (`v1=`); a delivery
/// is authentic if any candidate matches. Multiple `v1` entries occur during
/// secret rotation.
///
/// `now_unix` is injected (rather than read from the clock) so the timestamp
/// tolerance check is deterministically testable.
pub fn verify_signature(
    payload: &[u8],
    sig_header: &str,
    secret: &str,
    now_unix: i64,
    tolerance_secs: i64,
) -> Result<(), SignatureError> {
    let mut timestamp: Option<i64> = None;
    let mut v1_sigs: Vec<&str> = Vec::new();
    for part in sig_header.split(',') {
        let mut kv = part.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("t"), Some(v)) => timestamp = v.trim().parse().ok(),
            (Some("v1"), Some(v)) => v1_sigs.push(v.trim()),
            _ => {}
        }
    }

    let t = timestamp.ok_or(SignatureError::MalformedHeader)?;
    if v1_sigs.is_empty() {
        return Err(SignatureError::MalformedHeader);
    }
    if (now_unix - t).abs() > tolerance_secs {
        return Err(SignatureError::TimestampOutOfTolerance);
    }

    for sig in v1_sigs {
        // Skip non-hex candidates rather than failing — a single junk entry
        // shouldn't reject a delivery that also carries a valid signature.
        let Ok(sig_bytes) = hex::decode(sig) else {
            continue;
        };
        // `new_from_slice` accepts a key of any length, so this can't fail.
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
        mac.update(t.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        // `verify_slice` is a constant-time comparison.
        if mac.verify_slice(&sig_bytes).is_ok() {
            return Ok(());
        }
    }
    Err(SignatureError::NoMatchingSignature)
}

/// A parsed Stripe webhook event: just the fields we act on.
#[derive(Debug, Clone)]
pub struct WebhookEvent {
    /// Stripe event id (`evt_…`). Stable across redeliveries — the idempotency
    /// anchor for "have we already seen this event".
    pub id: String,
    /// e.g. `charge.dispute.created`, `charge.refunded`.
    pub event_type: String,
    /// `data.object` — the resource the event is about (a Dispute, a Charge…).
    pub object: serde_json::Value,
}

/// Parse the raw webhook body into a [`WebhookEvent`]. Call only after
/// [`verify_signature`] succeeds.
pub fn parse_event(payload: &[u8]) -> anyhow::Result<WebhookEvent> {
    let body: serde_json::Value = serde_json::from_slice(payload)
        .map_err(|e| anyhow::anyhow!("stripe webhook: invalid JSON body: {e}"))?;
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("stripe webhook: missing event id"))?
        .to_string();
    let event_type = body
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("stripe webhook: missing event type"))?
        .to_string();
    let object = body
        .get("data")
        .and_then(|d| d.get("object"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("stripe webhook: missing data.object"))?;
    Ok(WebhookEvent {
        id,
        event_type,
        object,
    })
}

/// What kind of clawback an event represents — surfaced as a metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClawbackKind {
    /// `charge.dispute.created` — card chargeback; the bank pulled the funds.
    Dispute,
    /// `charge.refunded` — a refund (dashboard- or API-initiated).
    Refund,
}

impl ClawbackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClawbackKind::Dispute => "dispute",
            ClawbackKind::Refund => "refund",
        }
    }
}

/// A credit debit demanded by a clawback event.
///
/// `amount_cents` is always positive — it is written as a *positive* customer
/// balance transaction, which reduces the customer's available credit (Stripe
/// represents credit as a negative balance).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clawback {
    pub kind: ClawbackKind,
    /// Positive cents to debit from the customer's credit balance.
    pub amount_cents: i64,
    /// Stripe customer id, when the event object carries it directly (charges
    /// do; disputes don't). When `None` the caller must resolve it from
    /// `charge_id` / `payment_intent_id`.
    pub customer: Option<String>,
    pub charge_id: Option<String>,
    pub payment_intent_id: Option<String>,
    /// Stable id of the underlying clawback object (refund id / dispute id, or
    /// the charge id as a fallback). Used as the Stripe `Idempotency-Key` on
    /// the debiting balance transaction so redeliveries and distinct partial
    /// refunds collapse / separate correctly.
    pub idempotency_anchor: String,
    pub description: String,
}

/// Interpret a verified event into the credit debit it implies, if any.
///
/// Returns `None` for events that need no balance change — notably
/// `payment_intent.payment_failed`: a failed payment never reached
/// `confirm_payment`, so no credit was ever granted to claw back.
///
/// Refund handling: a `charge.refunded` event carries the Charge, whose
/// `refunds.data` list (newest first) lets us debit *that* refund's amount and
/// anchor idempotency on its refund id — so two partial refunds produce two
/// distinct debits and a redelivery of either collapses. If the `refunds` list
/// isn't expanded in the payload we fall back to the cumulative
/// `amount_refunded` anchored on the charge id; this is exact for the common
/// single-/full-refund case and can only *under*-debit (never over-debit) in
/// the rare partial-refund-without-expansion case, where the second event's
/// charge-id anchor is deduped by Stripe.
pub fn interpret_event(event: &WebhookEvent) -> Option<Clawback> {
    let obj = &event.object;
    match event.event_type.as_str() {
        "charge.dispute.created" => {
            let amount = obj.get("amount").and_then(|v| v.as_i64())?;
            if amount <= 0 {
                return None;
            }
            let dispute_id = obj.get("id").and_then(|v| v.as_str())?.to_string();
            Some(Clawback {
                kind: ClawbackKind::Dispute,
                amount_cents: amount,
                customer: obj
                    .get("customer")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                charge_id: obj
                    .get("charge")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                payment_intent_id: obj
                    .get("payment_intent")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                description: format!("Chargeback debit for dispute {dispute_id}"),
                idempotency_anchor: dispute_id,
            })
        }
        "charge.refunded" => {
            let customer = obj
                .get("customer")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let charge_id = obj.get("id").and_then(|v| v.as_str()).map(str::to_string);
            let payment_intent_id = obj
                .get("payment_intent")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let latest_refund = obj
                .get("refunds")
                .and_then(|r| r.get("data"))
                .and_then(|d| d.as_array())
                .and_then(|arr| arr.first());

            let (amount, anchor) = match latest_refund {
                Some(refund) => {
                    let amt = refund.get("amount").and_then(|v| v.as_i64())?;
                    let anchor = refund
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .or_else(|| charge_id.clone())?;
                    (amt, anchor)
                }
                None => {
                    let amt = obj.get("amount_refunded").and_then(|v| v.as_i64())?;
                    (amt, charge_id.clone()?)
                }
            };
            if amount <= 0 {
                return None;
            }
            Some(Clawback {
                kind: ClawbackKind::Refund,
                amount_cents: amount,
                customer,
                charge_id,
                payment_intent_id,
                description: format!("Refund debit for {anchor}"),
                idempotency_anchor: anchor,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A known-answer vector computed with the same HMAC-SHA256 scheme Stripe
    // uses. `secret`, `payload`, and `t` are fixed; `sig` is HMAC(secret,
    // "{t}.{payload}"). If the construction ever drifts, this fails.
    const SECRET: &str = "whsec_test_secret";
    const PAYLOAD: &[u8] = b"{\"id\":\"evt_1\",\"type\":\"charge.refunded\"}";
    const TS: i64 = 1_700_000_000;

    fn sign(secret: &str, t: i64, payload: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(t.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verify_accepts_valid_signature() {
        let sig = sign(SECRET, TS, PAYLOAD);
        let header = format!("t={TS},v1={sig}");
        assert!(verify_signature(PAYLOAD, &header, SECRET, TS, DEFAULT_TOLERANCE_SECS).is_ok());
    }

    #[test]
    fn verify_accepts_within_tolerance() {
        let sig = sign(SECRET, TS, PAYLOAD);
        let header = format!("t={TS},v1={sig}");
        // now is 4 minutes after the signed timestamp — inside the 5-min window.
        let now = TS + 240;
        assert!(verify_signature(PAYLOAD, &header, SECRET, now, DEFAULT_TOLERANCE_SECS).is_ok());
    }

    #[test]
    fn verify_rejects_stale_timestamp() {
        let sig = sign(SECRET, TS, PAYLOAD);
        let header = format!("t={TS},v1={sig}");
        let now = TS + DEFAULT_TOLERANCE_SECS + 1;
        assert_eq!(
            verify_signature(PAYLOAD, &header, SECRET, now, DEFAULT_TOLERANCE_SECS),
            Err(SignatureError::TimestampOutOfTolerance)
        );
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        let sig = sign(SECRET, TS, PAYLOAD);
        let header = format!("t={TS},v1={sig}");
        let tampered = b"{\"id\":\"evt_1\",\"type\":\"charge.refunded\",\"x\":1}";
        assert_eq!(
            verify_signature(tampered, &header, SECRET, TS, DEFAULT_TOLERANCE_SECS),
            Err(SignatureError::NoMatchingSignature)
        );
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let sig = sign(SECRET, TS, PAYLOAD);
        let header = format!("t={TS},v1={sig}");
        assert_eq!(
            verify_signature(PAYLOAD, &header, "whsec_wrong", TS, DEFAULT_TOLERANCE_SECS),
            Err(SignatureError::NoMatchingSignature)
        );
    }

    #[test]
    fn verify_accepts_one_of_several_v1_during_rotation() {
        let good = sign(SECRET, TS, PAYLOAD);
        // Header carries a stale-secret signature first, then the valid one.
        let header = format!("t={TS},v1=deadbeef,v1={good}");
        assert!(verify_signature(PAYLOAD, &header, SECRET, TS, DEFAULT_TOLERANCE_SECS).is_ok());
    }

    #[test]
    fn verify_rejects_header_without_timestamp() {
        let sig = sign(SECRET, TS, PAYLOAD);
        let header = format!("v1={sig}");
        assert_eq!(
            verify_signature(PAYLOAD, &header, SECRET, TS, DEFAULT_TOLERANCE_SECS),
            Err(SignatureError::MalformedHeader)
        );
    }

    #[test]
    fn verify_rejects_header_without_v1() {
        let header = format!("t={TS}");
        assert_eq!(
            verify_signature(PAYLOAD, &header, SECRET, TS, DEFAULT_TOLERANCE_SECS),
            Err(SignatureError::MalformedHeader)
        );
    }

    #[test]
    fn parse_event_extracts_fields() {
        let body = br#"{"id":"evt_123","type":"charge.refunded","data":{"object":{"id":"ch_1"}}}"#;
        let event = parse_event(body).unwrap();
        assert_eq!(event.id, "evt_123");
        assert_eq!(event.event_type, "charge.refunded");
        assert_eq!(event.object.get("id").unwrap(), "ch_1");
    }

    #[test]
    fn parse_event_rejects_missing_data_object() {
        let body = br#"{"id":"evt_123","type":"charge.refunded"}"#;
        assert!(parse_event(body).is_err());
    }

    #[test]
    fn interpret_dispute_created_debits_full_amount() {
        let event = WebhookEvent {
            id: "evt_1".into(),
            event_type: "charge.dispute.created".into(),
            object: serde_json::json!({
                "id": "dp_1",
                "amount": 2500,
                "charge": "ch_1",
                "payment_intent": "pi_1",
            }),
        };
        let c = interpret_event(&event).unwrap();
        assert_eq!(c.kind, ClawbackKind::Dispute);
        assert_eq!(c.amount_cents, 2500);
        assert_eq!(c.customer, None); // disputes don't carry the customer
        assert_eq!(c.charge_id.as_deref(), Some("ch_1"));
        assert_eq!(c.payment_intent_id.as_deref(), Some("pi_1"));
        assert_eq!(c.idempotency_anchor, "dp_1");
    }

    #[test]
    fn interpret_refund_uses_latest_refund_amount_and_id() {
        // Partial second refund: newest is data[0]. We debit *its* amount (300),
        // not the cumulative 800, and anchor on its refund id.
        let event = WebhookEvent {
            id: "evt_2".into(),
            event_type: "charge.refunded".into(),
            object: serde_json::json!({
                "id": "ch_1",
                "customer": "cus_1",
                "payment_intent": "pi_1",
                "amount_refunded": 800,
                "refunds": { "data": [
                    { "id": "re_2", "amount": 300 },
                    { "id": "re_1", "amount": 500 },
                ]},
            }),
        };
        let c = interpret_event(&event).unwrap();
        assert_eq!(c.kind, ClawbackKind::Refund);
        assert_eq!(c.amount_cents, 300);
        assert_eq!(c.customer.as_deref(), Some("cus_1"));
        assert_eq!(c.idempotency_anchor, "re_2");
    }

    #[test]
    fn interpret_refund_falls_back_to_amount_refunded() {
        // No expanded refunds list — use cumulative amount_refunded + charge id.
        let event = WebhookEvent {
            id: "evt_3".into(),
            event_type: "charge.refunded".into(),
            object: serde_json::json!({
                "id": "ch_9",
                "customer": "cus_9",
                "amount_refunded": 1000,
            }),
        };
        let c = interpret_event(&event).unwrap();
        assert_eq!(c.amount_cents, 1000);
        assert_eq!(c.idempotency_anchor, "ch_9");
    }

    #[test]
    fn interpret_payment_failed_is_no_debit() {
        let event = WebhookEvent {
            id: "evt_4".into(),
            event_type: "payment_intent.payment_failed".into(),
            object: serde_json::json!({ "id": "pi_1", "amount": 5000 }),
        };
        assert!(interpret_event(&event).is_none());
    }

    #[test]
    fn interpret_ignores_unrelated_events() {
        let event = WebhookEvent {
            id: "evt_5".into(),
            event_type: "customer.created".into(),
            object: serde_json::json!({ "id": "cus_1" }),
        };
        assert!(interpret_event(&event).is_none());
    }

    #[test]
    fn interpret_refund_zero_amount_is_no_debit() {
        let event = WebhookEvent {
            id: "evt_6".into(),
            event_type: "charge.refunded".into(),
            object: serde_json::json!({
                "id": "ch_1",
                "customer": "cus_1",
                "refunds": { "data": [ { "id": "re_1", "amount": 0 } ] },
            }),
        };
        assert!(interpret_event(&event).is_none());
    }
}
