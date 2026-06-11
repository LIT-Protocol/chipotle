//! Email send + approval primitive (plan D6/M3) — the server half.
//!
//! Chipotle owns sending, nonce issuance, OTP step-up, and attestation
//! signing; the lit-actions runtime owns VERIFICATION (in-TEE, against the
//! pinned pubkey of this service's attestation key). A compromise of this
//! service can therefore deny approvals but cannot forge one.
//!
//! v1 stores pending approvals in memory: an approval must complete against
//! the instance that issued it, and a deploy/cutover drops pending (not yet
//! consumed) approvals. The attestation itself is stateless and verifiable
//! anywhere. A shared store is a documented follow-up, not a correctness fix.

use anyhow::{Context, Result, anyhow, bail};
use k256::ecdsa::signature::Signer;
use rand::RngCore;
use rocket::State;
use rocket::form::FromForm;
use rocket::http::ContentType;
use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ATTESTATION_SCHEMA: &str = "email-approval-v1";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim().trim_start_matches("0x");
    if s.len() % 2 != 0 {
        bail!("odd-length hex");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).context("invalid hex"))
        .collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex_encode(&Sha256::digest(data))
}

/// Light-weight email shape check — full RFC 5322 is a non-goal; Resend
/// rejects what slips through.
fn looks_like_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && s.len() <= 254
        && !s.chars().any(|c| c.is_whitespace() || c == ',')
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

impl ApprovalStatus {
    fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Denied => "denied",
            ApprovalStatus::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone)]
struct ApprovalRecord {
    /// sha256 of the requesting account's API key — scopes checkEmailApproval
    /// to the requester without storing the key itself.
    account_hash: String,
    to: String,
    summary: String,
    assurance: String,
    otp_hash: Option<String>,
    status: ApprovalStatus,
    expires_at_ms: u64,
    attestation: Option<String>,
    approved_at_ms: Option<u64>,
}

pub struct ApprovalConfig {
    /// Base URL rendered into approval links, e.g. https://test.chipotle.litprotocol.com
    pub public_base_url: String,
    pub resend_api_key: Option<String>,
    pub mail_from: String,
    /// Per-account emails (notifications + approval requests) per UTC day.
    pub email_daily_quota: u32,
    /// Dev/e2e affordance: include the approval URL in the op response so a
    /// test can "click" the link without an inbox. NEVER enable in prod — the
    /// requester being able to open the link collapses the email factor.
    pub expose_links: bool,
    pub max_ttl_sec: u32,
}

impl ApprovalConfig {
    pub fn from_env() -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self {
            public_base_url: env("LIT_APPROVAL_PUBLIC_BASE_URL")
                .unwrap_or_else(|| "http://localhost:8000".to_string())
                .trim_end_matches('/')
                .to_string(),
            resend_api_key: env("RESEND_API_KEY"),
            mail_from: env("LIT_APPROVAL_MAIL_FROM")
                .unwrap_or_else(|| "Lit Actions <approvals@actions.litprotocol.com>".to_string()),
            email_daily_quota: env("LIT_EMAIL_DAILY_QUOTA")
                .and_then(|v| v.parse().ok())
                .unwrap_or(50),
            expose_links: env("LIT_APPROVAL_EXPOSE_LINK").is_some_and(|v| v == "true" || v == "1"),
            max_ttl_sec: env("LIT_APPROVAL_MAX_TTL_SEC")
                .and_then(|v| v.parse().ok())
                .unwrap_or(7 * 24 * 3600),
        }
    }
}

pub struct ApprovalService {
    cfg: ApprovalConfig,
    signing_key: k256::ecdsa::SigningKey,
    link_key: [u8; 32],
    store: Mutex<HashMap<String, ApprovalRecord>>,
    /// account_hash → (utc_day, emails sent today)
    quotas: Mutex<HashMap<String, (u64, u32)>>,
    http: reqwest::Client,
}

pub struct RequestedApproval {
    pub approval_id: String,
    pub otp: Option<String>,
    pub approval_url: Option<String>,
}

// Manual impl: the signing key, link key, and store contents must never reach
// debug output.
impl std::fmt::Debug for ApprovalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalService")
            .field("attestation_pubkey", &self.attestation_pubkey_hex())
            .finish_non_exhaustive()
    }
}

impl ApprovalService {
    /// Construct from env. The attestation key comes from
    /// LIT_APPROVAL_ATTESTATION_KEY (32-byte hex); without it an ephemeral key
    /// is generated and a loud warning logged — approvals then verify only
    /// against runtimes pinned to the printed ephemeral pubkey (dev only).
    pub fn from_env() -> Result<Self> {
        let cfg = ApprovalConfig::from_env();
        let signing_key = match std::env::var("LIT_APPROVAL_ATTESTATION_KEY") {
            Ok(hex) if !hex.trim().is_empty() => {
                let bytes = hex_decode(&hex).context("LIT_APPROVAL_ATTESTATION_KEY")?;
                k256::ecdsa::SigningKey::from_slice(&bytes)
                    .context("LIT_APPROVAL_ATTESTATION_KEY must be a 32-byte secp256k1 scalar")?
            }
            _ => {
                let mut bytes = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut bytes);
                let key = k256::ecdsa::SigningKey::from_slice(&bytes).expect("32 random bytes");
                tracing::warn!(
                    pubkey = hex_encode(key.verifying_key().to_encoded_point(true).as_bytes()),
                    "LIT_APPROVAL_ATTESTATION_KEY not set — generated an EPHEMERAL attestation key; \
                     approvals will not survive restarts and runtimes must pin the pubkey above (dev only)"
                );
                key
            }
        };
        // Link tokens only need to be unforgeable by outsiders; deriving from
        // the attestation key keeps configuration to a single secret.
        let mut link_key = [0u8; 32];
        link_key.copy_from_slice(&Sha256::digest(
            [signing_key.to_bytes().as_slice(), b"lit-approval-link-key"].concat(),
        ));
        Ok(Self {
            cfg,
            signing_key,
            link_key,
            store: Mutex::new(HashMap::new()),
            quotas: Mutex::new(HashMap::new()),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .context("building Resend HTTP client")?,
        })
    }

    /// Hex of the SEC1-compressed attestation pubkey — pin this on the
    /// runtime as LIT_APPROVAL_ATTESTATION_PUBKEY.
    pub fn attestation_pubkey_hex(&self) -> String {
        hex_encode(self.signing_key.verifying_key().to_encoded_point(true).as_bytes())
    }

    fn account_hash(api_key: &str) -> String {
        sha256_hex(api_key.as_bytes())[..16].to_string()
    }

    fn link_token(&self, approval_id: &str, to: &str, expires_at_ms: u64) -> String {
        sha256_hex(
            &[
                self.link_key.as_slice(),
                approval_id.as_bytes(),
                b"|",
                to.as_bytes(),
                b"|",
                expires_at_ms.to_string().as_bytes(),
            ]
            .concat(),
        )
    }

    fn check_quota(&self, account_hash: &str) -> Result<()> {
        let day = now_ms() / 86_400_000;
        let mut quotas = self.quotas.lock().unwrap_or_else(|e| e.into_inner());
        let entry = quotas.entry(account_hash.to_string()).or_insert((day, 0));
        if entry.0 != day {
            *entry = (day, 0);
        }
        if entry.1 >= self.cfg.email_daily_quota {
            bail!(
                "email quota exceeded ({}/day per account) — contact support to raise it",
                self.cfg.email_daily_quota
            );
        }
        entry.1 += 1;
        Ok(())
    }

    async fn deliver(&self, to: &str, subject: &str, text: &str) -> Result<()> {
        let Some(api_key) = self.cfg.resend_api_key.as_deref() else {
            // Dev mode: no provider configured. Log (without the body — it may
            // embed an approval link) and treat as delivered.
            tracing::warn!(to, subject, "RESEND_API_KEY not set — email NOT sent (dev mode)");
            return Ok(());
        };
        #[derive(serde::Serialize)]
        struct SendReq<'a> {
            from: &'a str,
            to: [&'a str; 1],
            subject: &'a str,
            text: &'a str,
        }
        let resp = self
            .http
            .post("https://api.resend.com/emails")
            .bearer_auth(api_key)
            .json(&SendReq {
                from: &self.cfg.mail_from,
                to: [to],
                subject,
                text,
            })
            .send()
            .await
            .context("Resend HTTP request failed")?;
        if !resp.status().is_success() {
            bail!("Resend returned {}", resp.status());
        }
        Ok(())
    }

    /// `Lit.Actions.sendEmail` — plain notification, strict template.
    pub async fn send_email(&self, api_key: &str, to: &str, subject: &str, text: &str) -> Result<()> {
        if !looks_like_email(to) {
            bail!("invalid recipient address");
        }
        if subject.len() > 200 || text.len() > 10_000 {
            bail!("subject must be <= 200 chars and text <= 10000 chars");
        }
        let account = Self::account_hash(api_key);
        self.check_quota(&account)?;
        // Fixed prefix so an action cannot impersonate Lit system mail.
        let subject = format!("[Lit Action] {subject}");
        self.deliver(to, &subject, text).await
    }

    /// `Lit.Actions.requestEmailApproval` — issue id (+ OTP for L2), email the link.
    pub async fn request_approval(
        &self,
        api_key: &str,
        to: &str,
        summary: &str,
        assurance: &str,
        ttl_sec: u32,
    ) -> Result<RequestedApproval> {
        if !looks_like_email(to) {
            bail!("invalid approver address");
        }
        if summary.trim().is_empty() || summary.len() > 500 {
            bail!("summary must be 1..=500 chars");
        }
        match assurance {
            "L1" | "L2" => {}
            "L3" => bail!("assurance L3 (chain co-sign) is not available yet — use L2"),
            other => bail!("unknown assurance level {other:?} (use L1 or L2)"),
        }
        let ttl_sec = ttl_sec.clamp(60, self.cfg.max_ttl_sec);

        let account = Self::account_hash(api_key);
        self.check_quota(&account)?;

        let mut id_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut id_bytes);
        let approval_id = format!("apr_{}", hex_encode(&id_bytes));

        let otp = if assurance == "L2" {
            let mut otp_bytes = [0u8; 4];
            rand::thread_rng().fill_bytes(&mut otp_bytes);
            Some(format!("{:06}", u32::from_be_bytes(otp_bytes) % 1_000_000))
        } else {
            None
        };

        let expires_at_ms = now_ms() + u64::from(ttl_sec) * 1000;
        let token = self.link_token(&approval_id, to, expires_at_ms);
        let url = format!("{}/approvals/{}?t={}", self.cfg.public_base_url, approval_id, token);

        {
            let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
            // Opportunistic sweep keeps the in-memory map bounded without a
            // background task.
            store.retain(|_, r| r.expires_at_ms + 86_400_000 > now_ms());
            store.insert(
                approval_id.clone(),
                ApprovalRecord {
                    account_hash: account,
                    to: to.to_string(),
                    summary: summary.to_string(),
                    assurance: assurance.to_string(),
                    otp_hash: otp.as_deref().map(|o| sha256_hex(o.as_bytes())),
                    status: ApprovalStatus::Pending,
                    expires_at_ms,
                    attestation: None,
                    approved_at_ms: None,
                },
            );
        }

        let minutes = ttl_sec / 60;
        let step_up = if assurance == "L2" {
            "\nYou will be asked for the 6-digit code shown in the requesting app."
        } else {
            ""
        };
        let text = format!(
            "A Lit Action is requesting your approval:\n\n  {summary}\n\nReview and decide here (link expires in {minutes} minutes):\n{url}\n{step_up}\nIf you did not expect this request, you can ignore this email or open the link and deny it."
        );
        self.deliver(to, "[Lit] Approval requested", &text).await?;

        Ok(RequestedApproval {
            approval_id,
            otp,
            approval_url: self.cfg.expose_links.then_some(url),
        })
    }

    /// `Lit.Actions.checkEmailApproval` — status + attestation, scoped to the requesting account.
    pub fn check(&self, api_key: &str, approval_id: &str) -> Result<(String, Option<String>)> {
        let account = Self::account_hash(api_key);
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let Some(record) = store.get_mut(approval_id) else {
            bail!("unknown approvalId (it may have expired and been swept, or belong to another instance)");
        };
        if record.account_hash != account {
            // Don't leak existence across accounts.
            bail!("unknown approvalId (it may have expired and been swept, or belong to another instance)");
        }
        if record.status == ApprovalStatus::Pending && record.expires_at_ms <= now_ms() {
            record.status = ApprovalStatus::Expired;
        }
        Ok((record.status.as_str().to_string(), record.attestation.clone()))
    }

    fn sign_attestation(&self, payload_json: &str) -> String {
        let sig: k256::ecdsa::Signature = self.signing_key.sign(payload_json.as_bytes());
        serde_json::json!({
            "v": ATTESTATION_SCHEMA,
            "alg": "secp256k1-sha256",
            "payload": payload_json,
            "sig": hex_encode(&sig.to_bytes()),
        })
        .to_string()
    }

    /// Data needed to render the approval page; validates the link token.
    pub fn page_data(&self, approval_id: &str, token: &str) -> Result<(String, String, bool)> {
        let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let record = store.get(approval_id).ok_or_else(|| anyhow!("unknown or expired approval"))?;
        let expected = self.link_token(approval_id, &record.to, record.expires_at_ms);
        if token != expected {
            bail!("invalid approval link");
        }
        if record.status != ApprovalStatus::Pending {
            bail!("this approval is already {}", record.status.as_str());
        }
        if record.expires_at_ms <= now_ms() {
            bail!("this approval has expired");
        }
        Ok((
            record.summary.clone(),
            record.assurance.clone(),
            record.otp_hash.is_some(),
        ))
    }

    /// Handle the approver's decision. Single-use: only a Pending record can
    /// transition. On approve, signs and stores the attestation.
    pub fn decide(&self, approval_id: &str, token: &str, otp: Option<&str>, approve: bool) -> Result<String> {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let record = store
            .get_mut(approval_id)
            .ok_or_else(|| anyhow!("unknown or expired approval"))?;
        let expected = self.link_token(approval_id, &record.to, record.expires_at_ms);
        if token != expected {
            bail!("invalid approval link");
        }
        if record.expires_at_ms <= now_ms() {
            record.status = ApprovalStatus::Expired;
            bail!("this approval has expired");
        }
        if record.status != ApprovalStatus::Pending {
            bail!("this approval is already {}", record.status.as_str());
        }
        if !approve {
            record.status = ApprovalStatus::Denied;
            return Ok("denied".to_string());
        }
        if let Some(expected_otp_hash) = &record.otp_hash {
            let supplied = otp.unwrap_or_default().trim();
            if supplied.is_empty() || &sha256_hex(supplied.as_bytes()) != expected_otp_hash {
                bail!("incorrect or missing code — enter the 6-digit code from the requesting app");
            }
        }
        let approved_at_ms = now_ms();
        let payload = serde_json::json!({
            "schema": ATTESTATION_SCHEMA,
            "approval_id": approval_id,
            "approver": record.to,
            "assurance": record.assurance,
            "status": "approved",
            "approved_at_ms": approved_at_ms,
            "expires_at_ms": record.expires_at_ms,
        })
        .to_string();
        record.attestation = Some(self.sign_attestation(&payload));
        record.approved_at_ms = Some(approved_at_ms);
        record.status = ApprovalStatus::Approved;
        Ok("approved".to_string())
    }
}

// ---------------------------------------------------------------------------
// Routes (mounted at "/" — approval links are user-facing)

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn page(title: &str, body: &str) -> RawHtml<String> {
    RawHtml(format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
body{{font-family:-apple-system,system-ui,sans-serif;background:#0b0e14;color:#e6e6e6;display:flex;justify-content:center;padding:48px 16px}}
.card{{max-width:520px;width:100%;background:#141925;border:1px solid #232a3b;border-radius:12px;padding:32px}}
h1{{font-size:18px;margin:0 0 8px}} p{{line-height:1.5;color:#b8c0d0}}
.summary{{background:#0b0e14;border:1px solid #232a3b;border-radius:8px;padding:16px;margin:16px 0;white-space:pre-wrap}}
input[type=text]{{width:100%;padding:10px;border-radius:8px;border:1px solid #232a3b;background:#0b0e14;color:#e6e6e6;font-size:18px;letter-spacing:4px;text-align:center}}
button{{padding:12px 20px;border-radius:8px;border:0;font-size:15px;cursor:pointer;margin-right:8px}}
.approve{{background:#22c55e;color:#06270f}} .deny{{background:#232a3b;color:#e6e6e6}}
.muted{{font-size:13px;color:#6b7280}}
</style></head><body><div class="card">{body}</div></body></html>"#
    ))
}

#[rocket::get("/approvals/<id>?<t>")]
pub async fn approval_page(svc: &State<Arc<ApprovalService>>, id: &str, t: &str) -> RawHtml<String> {
    match svc.page_data(id, t) {
        Ok((summary, assurance, needs_otp)) => {
            let otp_field = if needs_otp {
                r#"<p>Enter the 6-digit code shown in the requesting app (assurance L2 — the email link alone is not enough to approve):</p>
                   <p><input type="text" name="otp" inputmode="numeric" autocomplete="one-time-code" maxlength="6" placeholder="••••••"></p>"#
            } else {
                ""
            };
            let body = format!(
                r#"<h1>Approval requested</h1>
<p>A Lit Action is asking you to approve:</p>
<div class="summary">{}</div>
<form method="post" action="/approvals/{}">
  <input type="hidden" name="t" value="{}">
  {}
  <button class="approve" name="decision" value="approve" type="submit">Approve</button>
  <button class="deny" name="decision" value="deny" type="submit">Deny</button>
</form>
<p class="muted">Assurance level {} · This link is single-use. If you did not expect this request, deny it.</p>"#,
                html_escape(&summary),
                html_escape(id),
                html_escape(t),
                otp_field,
                html_escape(&assurance),
            );
            page("Lit approval", &body)
        }
        Err(e) => page(
            "Lit approval",
            &format!("<h1>Unavailable</h1><p>{}</p>", html_escape(&e.to_string())),
        ),
    }
}

#[derive(FromForm)]
pub struct DecisionForm {
    t: String,
    otp: Option<String>,
    decision: String,
}

#[rocket::post("/approvals/<id>", data = "<form>")]
pub async fn approval_decide(
    svc: &State<Arc<ApprovalService>>,
    id: &str,
    form: rocket::form::Form<DecisionForm>,
) -> RawHtml<String> {
    let approve = form.decision == "approve";
    match svc.decide(id, &form.t, form.otp.as_deref(), approve) {
        Ok(outcome) if outcome == "approved" => page(
            "Approved",
            "<h1>Approved ✓</h1><p>The action can now verify this approval and proceed. You can close this window.</p>",
        ),
        Ok(_) => page(
            "Denied",
            "<h1>Denied</h1><p>The request was denied; the action will not proceed. You can close this window.</p>",
        ),
        Err(e) => page(
            "Lit approval",
            &format!("<h1>Not completed</h1><p>{}</p>", html_escape(&e.to_string())),
        ),
    }
}

/// The attestation pubkey runtimes must pin (LIT_APPROVAL_ATTESTATION_PUBKEY).
#[rocket::get("/approvals_pubkey")]
pub async fn approval_pubkey(svc: &State<Arc<ApprovalService>>) -> (ContentType, Json<serde_json::Value>) {
    (
        ContentType::JSON,
        Json(serde_json::json!({
            "schema": ATTESTATION_SCHEMA,
            "alg": "secp256k1-sha256",
            "pubkey": svc.attestation_pubkey_hex(),
        })),
    )
}

pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![approval_page, approval_decide, approval_pubkey]
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::signature::Verifier;

    fn svc() -> ApprovalService {
        // No env reliance: ephemeral key, no resend → deliver() is a no-op.
        let signing_key = k256::ecdsa::SigningKey::from_slice(&[7u8; 32]).unwrap();
        let mut link_key = [0u8; 32];
        link_key.copy_from_slice(&Sha256::digest(
            [signing_key.to_bytes().as_slice(), b"lit-approval-link-key"].concat(),
        ));
        ApprovalService {
            cfg: ApprovalConfig {
                public_base_url: "http://localhost:8000".into(),
                resend_api_key: None,
                mail_from: "t@t.io".into(),
                email_daily_quota: 3,
                expose_links: true,
                max_ttl_sec: 3600,
            },
            signing_key,
            link_key,
            store: Mutex::new(HashMap::new()),
            quotas: Mutex::new(HashMap::new()),
            http: reqwest::Client::new(),
        }
    }

    #[tokio::test]
    async fn l2_flow_approve_with_otp_signs_verifiable_attestation() {
        let s = svc();
        let req = s
            .request_approval("key1", "cfo@example.com", "Sweep 2.5 BTC", "L2", 600)
            .await
            .unwrap();
        let otp = req.otp.clone().unwrap();
        let url = req.approval_url.unwrap();
        let token = url.split("t=").nth(1).unwrap().to_string();

        // pending before decision
        let (status, att) = s.check("key1", &req.approval_id).unwrap();
        assert_eq!(status, "pending");
        assert!(att.is_none());

        // wrong OTP refused, record stays pending (retryable)
        assert!(s.decide(&req.approval_id, &token, Some("000000"), true).is_err());
        // missing OTP refused
        assert!(s.decide(&req.approval_id, &token, None, true).is_err());
        // bad token refused
        assert!(s.decide(&req.approval_id, "deadbeef", Some(&otp), true).is_err());

        // correct OTP approves
        assert_eq!(s.decide(&req.approval_id, &token, Some(&otp), true).unwrap(), "approved");
        let (status, att) = s.check("key1", &req.approval_id).unwrap();
        assert_eq!(status, "approved");
        let att = att.unwrap();

        // attestation verifies against the service pubkey and binds the id
        let env: serde_json::Value = serde_json::from_str(&att).unwrap();
        let payload = env["payload"].as_str().unwrap();
        let sig_bytes = hex_decode(env["sig"].as_str().unwrap()).unwrap();
        let sig = k256::ecdsa::Signature::from_slice(&sig_bytes).unwrap();
        s.signing_key
            .verifying_key()
            .verify(payload.as_bytes(), &sig)
            .expect("attestation must verify");
        let p: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(p["approval_id"], req.approval_id.as_str());
        assert_eq!(p["assurance"], "L2");
        assert_eq!(p["status"], "approved");

        // single-use: second decision refused
        assert!(s.decide(&req.approval_id, &token, Some(&otp), true).is_err());
    }

    #[tokio::test]
    async fn l1_flow_deny_and_account_scoping() {
        let s = svc();
        let req = s
            .request_approval("key1", "ops@example.com", "Run weekly report", "L1", 600)
            .await
            .unwrap();
        assert!(req.otp.is_none(), "L1 has no OTP step-up");
        let token = req.approval_url.unwrap().split("t=").nth(1).unwrap().to_string();

        // another account cannot see the approval
        assert!(s.check("other-key", &req.approval_id).is_err());

        assert_eq!(s.decide(&req.approval_id, &token, None, false).unwrap(), "denied");
        let (status, att) = s.check("key1", &req.approval_id).unwrap();
        assert_eq!(status, "denied");
        assert!(att.is_none(), "denied approvals carry no attestation");
    }

    #[tokio::test]
    async fn quota_and_validation() {
        let s = svc();
        // quota of 3/day
        for _ in 0..3 {
            s.send_email("key1", "a@b.co", "s", "t").await.unwrap();
        }
        assert!(s.send_email("key1", "a@b.co", "s", "t").await.is_err());
        // other account unaffected
        s.send_email("key2", "a@b.co", "s", "t").await.unwrap();

        assert!(s.request_approval("k", "not-an-email", "x", "L1", 600).await.is_err());
        assert!(s.request_approval("k", "a@b.co", "", "L1", 600).await.is_err());
        assert!(s.request_approval("k", "a@b.co", "x", "L9", 600).await.is_err());
        assert!(
            s.request_approval("k", "a@b.co", "x", "L3", 600).await.is_err(),
            "L3 is explicitly not available yet"
        );
    }

    #[tokio::test]
    async fn expired_approvals_report_expired() {
        let s = svc();
        let req = s
            .request_approval("key1", "cfo@example.com", "x", "L1", 600)
            .await
            .unwrap();
        // force expiry
        {
            let mut store = s.store.lock().unwrap();
            store.get_mut(&req.approval_id).unwrap().expires_at_ms = 1;
        }
        let (status, _) = s.check("key1", &req.approval_id).unwrap();
        assert_eq!(status, "expired");
        let token = req.approval_url.unwrap().split("t=").nth(1).unwrap().to_string();
        assert!(s.decide(&req.approval_id, &token, None, true).is_err());
    }
}
