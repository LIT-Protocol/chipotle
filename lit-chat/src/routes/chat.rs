//! Chat CRUD + the SSE streaming proxy (plans/tee-chat-app.md sections 4.1,
//! 4.2). All content is decrypted only inside the request path for the
//! authenticated owner and never logged.

use crate::config::Config;
use crate::crypto::keys::KeySource;
use crate::identity::UserKind;
use crate::metering;
use crate::openrouter::{ChatMessage, OpenRouterClient, StreamEvent};
use crate::rate_limit::StreamRateLimit;
use crate::routes::{err, ApiError, ChatUser, Csrf, RuntimeKeyCache};
use crate::session;
use crate::store::{conversations, messages, meters, settings, users};
use crate::{envelope, user_kek, Keyring};
use rocket::http::Status;
use rocket::response::stream::{Event, EventStream};
use rocket::serde::json::Json;
use rocket::{delete, get, patch, post, State};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

const SYSTEM_PROMPT: &str = "You are Lit Chat, a private AI assistant served from a trusted \
execution environment. Be helpful and concise.";
const AUTO_TITLE_MAX_CHARS: usize = 60;

fn internal(e: impl std::fmt::Display, what: &str) -> ApiError {
    tracing::warn!("{what}: {e}");
    err(Status::InternalServerError, "server_error")
}

// ---------------------------------------------------------------------------
// Session/context

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub kind: &'static str,
    pub csrf: String,
    pub anon_daily_token_budget: i64,
    pub anon_tokens_used_today: i64,
}

#[get("/api/me")]
pub async fn me(
    user: ChatUser,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    keyring: &State<Keyring>,
    key_source: &State<KeySource>,
) -> Result<Json<MeResponse>, ApiError> {
    let mut used = 0;
    if user.kind == UserKind::Anon {
        let kek = user_kek(key_source, &user.user_ref)
            .await
            .map_err(|e| internal(e, "kek derive"))?;
        if let Ok((state, _)) = metering::load(pool, &kek, &user.user_ref_hash).await {
            if state.day == metering::today_utc() {
                used = state.day_tokens;
            }
        }
    }
    Ok(Json(MeResponse {
        kind: user.kind.as_str(),
        csrf: session::csrf_for(&keyring.session_mac, &user.sid),
        anon_daily_token_budget: cfg.anon_daily_token_budget,
        anon_tokens_used_today: used,
    }))
}

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub display_name: String,
    pub zdr: bool,
    pub prompt_usd_per_mtok: Option<f64>,
    pub completion_usd_per_mtok: Option<f64>,
}

type ModelRow = (String, String, bool, Option<f64>, Option<f64>);

#[get("/api/models")]
pub async fn models(pool: &State<PgPool>) -> Result<Json<Vec<ModelInfo>>, ApiError> {
    let rows: Vec<ModelRow> = sqlx::query_as(
        "SELECT model_id, display_name, zdr, prompt_usd_per_mtok, completion_usd_per_mtok
         FROM model_catalog WHERE enabled ORDER BY display_name",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| internal(e, "model catalog"))?;
    Ok(Json(
        rows.into_iter()
            .map(|(model_id, display_name, zdr, p, c)| ModelInfo {
                model_id,
                display_name,
                zdr,
                prompt_usd_per_mtok: p,
                completion_usd_per_mtok: c,
            })
            .collect(),
    ))
}

async fn model_allowed(pool: &PgPool, model_id: &str) -> bool {
    let row: Result<Option<(String,)>, _> =
        sqlx::query_as("SELECT model_id FROM model_catalog WHERE model_id = $1 AND enabled")
            .bind(model_id)
            .fetch_optional(pool)
            .await;
    matches!(row, Ok(Some(_)))
}

// ---------------------------------------------------------------------------
// Conversations

#[derive(Debug, Serialize)]
pub struct ConversationInfo {
    pub id: Uuid,
    pub title: Option<String>,
    pub model_id: String,
    pub version: i64,
    pub updated_at: String,
}

#[get("/api/conversations")]
pub async fn list_conversations(
    user: ChatUser,
    pool: &State<PgPool>,
    key_source: &State<KeySource>,
) -> Result<Json<Vec<ConversationInfo>>, ApiError> {
    let kek = user_kek(key_source, &user.user_ref)
        .await
        .map_err(|e| internal(e, "kek derive"))?;
    let rows = conversations::list_for_user(pool, &user.user_ref_hash)
        .await
        .map_err(|e| internal(e, "list conversations"))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let title = match (
            &row.enc_title,
            envelope::unwrap_dek(&kek, &row.wrapped_dek, &user.user_ref_hash, row.id),
        ) {
            (Some(enc), Ok(dek)) => envelope::decrypt_title(&dek, row.id, enc).ok(),
            _ => None,
        };
        out.push(ConversationInfo {
            id: row.id,
            title,
            model_id: row.model_id,
            version: row.version,
            updated_at: row
                .updated_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        });
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct CreateConversationBody {
    pub model_id: String,
}

#[post("/api/conversations", format = "json", data = "<body>")]
pub async fn create_conversation(
    user: ChatUser,
    _csrf: Csrf,
    body: Json<CreateConversationBody>,
    pool: &State<PgPool>,
    key_source: &State<KeySource>,
) -> Result<Json<ConversationInfo>, ApiError> {
    if !model_allowed(pool, &body.model_id).await {
        return Err(err(Status::BadRequest, "unknown_model"));
    }
    let kek = user_kek(key_source, &user.user_ref)
        .await
        .map_err(|e| internal(e, "kek derive"))?;
    let id = Uuid::new_v4();
    let dek = envelope::mint_dek();
    let wrapped = envelope::wrap_dek(&kek, &dek, &user.user_ref_hash, id)
        .map_err(|e| internal(e, "dek wrap"))?;
    conversations::create(pool, id, &user.user_ref_hash, &wrapped, &body.model_id)
        .await
        .map_err(|e| internal(e, "create conversation"))?;
    Ok(Json(ConversationInfo {
        id,
        title: None,
        model_id: body.model_id.clone(),
        version: 1,
        updated_at: String::new(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RenameBody {
    pub title: String,
    pub expected_version: i64,
}

#[patch("/api/conversations/<id>", format = "json", data = "<body>")]
pub async fn rename_conversation(
    user: ChatUser,
    _csrf: Csrf,
    id: Uuid,
    body: Json<RenameBody>,
    pool: &State<PgPool>,
    key_source: &State<KeySource>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.title.is_empty() || body.title.chars().count() > 200 {
        return Err(err(Status::BadRequest, "bad_title"));
    }
    let Some(row) = conversations::get_owned(pool, id, &user.user_ref_hash)
        .await
        .map_err(|e| internal(e, "get conversation"))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    let kek = user_kek(key_source, &user.user_ref)
        .await
        .map_err(|e| internal(e, "kek derive"))?;
    let dek = envelope::unwrap_dek(&kek, &row.wrapped_dek, &user.user_ref_hash, id)
        .map_err(|e| internal(e, "dek unwrap"))?;
    let enc =
        envelope::encrypt_title(&dek, id, &body.title).map_err(|e| internal(e, "title encrypt"))?;
    match conversations::set_title(pool, id, &user.user_ref_hash, &enc, body.expected_version)
        .await
        .map_err(|e| internal(e, "set title"))?
    {
        Some(version) => Ok(Json(json!({"ok": true, "version": version}))),
        None => Err(err(Status::Conflict, "version_conflict")),
    }
}

#[delete("/api/conversations/<id>")]
pub async fn delete_conversation(
    user: ChatUser,
    _csrf: Csrf,
    id: Uuid,
    pool: &State<PgPool>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Hard delete, immediately. Encrypted copies persist in DB backups until
    // backup expiry; disclosed in the privacy panel, and we never say
    // "crypto-shred" (the KEK is re-derivable by design).
    let deleted = conversations::delete(pool, id, &user.user_ref_hash)
        .await
        .map_err(|e| internal(e, "delete conversation"))?;
    if deleted {
        Ok(Json(json!({"ok": true})))
    } else {
        Err(err(Status::NotFound, "not_found"))
    }
}

#[derive(Debug, Serialize)]
pub struct MessageInfo {
    pub id: Uuid,
    pub seq: i64,
    pub role: String,
    pub content: String,
}

#[get("/api/conversations/<id>/messages")]
pub async fn list_messages(
    user: ChatUser,
    id: Uuid,
    pool: &State<PgPool>,
    key_source: &State<KeySource>,
) -> Result<Json<Vec<MessageInfo>>, ApiError> {
    let Some(row) = conversations::get_owned(pool, id, &user.user_ref_hash)
        .await
        .map_err(|e| internal(e, "get conversation"))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    let kek = user_kek(key_source, &user.user_ref)
        .await
        .map_err(|e| internal(e, "kek derive"))?;
    let dek = envelope::unwrap_dek(&kek, &row.wrapped_dek, &user.user_ref_hash, id)
        .map_err(|e| internal(e, "dek unwrap"))?;
    let rows = messages::list(pool, id)
        .await
        .map_err(|e| internal(e, "list messages"))?;
    let mut out = Vec::with_capacity(rows.len());
    for m in rows {
        let content = envelope::decrypt_message(&dek, id, m.id, m.seq, &m.role, &m.ciphertext)
            .map_err(|e| internal(e, "message decrypt"))?;
        out.push(MessageInfo {
            id: m.id,
            seq: m.seq,
            role: m.role,
            content,
        });
    }
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Streaming

#[derive(Debug, Deserialize)]
pub struct StreamBody {
    pub conversation_id: Uuid,
    /// Absent (or empty) with regenerate=true: re-answer the last user turn.
    pub content: Option<String>,
    #[serde(default)]
    pub regenerate: bool,
    /// Optional model switch, persisted on the conversation.
    pub model_id: Option<String>,
}

const MAX_MESSAGE_CHARS: usize = 32_000;

#[allow(clippy::too_many_arguments)] // Rocket guards + managed state are all parameters.
#[post("/api/chat/stream", format = "json", data = "<body>")]
pub async fn stream(
    user: ChatUser,
    _csrf: Csrf,
    _rate: StreamRateLimit,
    body: Json<StreamBody>,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    key_source: &State<KeySource>,
    or_client: &State<OpenRouterClient>,
    keyring: &State<Keyring>,
    key_cache: &State<RuntimeKeyCache>,
) -> Result<EventStream![Event + 'static], ApiError> {
    let body = body.into_inner();
    let conv_id = body.conversation_id;
    let Some(conv) = conversations::get_owned(pool, conv_id, &user.user_ref_hash)
        .await
        .map_err(|e| internal(e, "get conversation"))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };

    let kek = user_kek(key_source, &user.user_ref)
        .await
        .map_err(|e| internal(e, "kek derive"))?;
    let dek = envelope::unwrap_dek(&kek, &conv.wrapped_dek, &user.user_ref_hash, conv_id)
        .map_err(|e| internal(e, "dek unwrap"))?;

    // Reserve-before shape: breaker + caller budget, before any upstream call.
    let decision = metering::check_budget(pool, cfg, &kek, &user.user_ref_hash, user.kind)
        .await
        .map_err(|e| internal(e, "budget check"))?;
    if !decision.allowed {
        return Err(match decision.reason {
            "anon_daily_budget_exhausted" => err(Status::PaymentRequired, decision.reason),
            _ => err(Status::ServiceUnavailable, decision.reason),
        });
    }

    // Model selection (optionally switching the conversation's model).
    let mut model_id = conv.model_id.clone();
    if let Some(m) = &body.model_id {
        if m != &model_id {
            if !model_allowed(pool, m).await {
                return Err(err(Status::BadRequest, "unknown_model"));
            }
            sqlx::query(
                "UPDATE conversations SET model_id = $1 WHERE id = $2 AND user_ref_hash = $3",
            )
            .bind(m)
            .bind(conv_id)
            .bind(&user.user_ref_hash)
            .execute(pool.inner())
            .await
            .map_err(|e| internal(e, "model switch"))?;
            model_id = m.clone();
        }
    }

    let Some((api_key, _key_id)) = key_cache
        .get(pool, &keyring.provider_kek)
        .await
        .map_err(|e| internal(e, "runtime key load"))?
    else {
        return Err(err(Status::ServiceUnavailable, "no_inference_key"));
    };

    // Regenerate: drop the trailing assistant turn instead of appending.
    if body.regenerate {
        messages::pop_trailing_assistant(pool, conv_id)
            .await
            .map_err(|e| internal(e, "pop assistant"))?;
    }

    // Persist the user message before calling upstream, so a mid-stream crash
    // never loses the user's own words.
    let mut user_msg: Option<(Uuid, i64, String)> = None;
    if !body.regenerate {
        let content = body.content.clone().unwrap_or_default();
        if content.trim().is_empty() {
            return Err(err(Status::BadRequest, "empty_message"));
        }
        if content.chars().count() > MAX_MESSAGE_CHARS {
            return Err(err(Status::BadRequest, "message_too_long"));
        }
        let msg_id = Uuid::new_v4();
        let seq = messages::next_seq(pool, conv_id)
            .await
            .map_err(|e| internal(e, "next seq"))?;
        let ct = envelope::encrypt_message(&dek, conv_id, msg_id, seq, "user", &content)
            .map_err(|e| internal(e, "message encrypt"))?;
        messages::append(pool, msg_id, conv_id, seq, "user", &ct, None)
            .await
            .map_err(|e| internal(e, "append user message"))?;
        user_msg = Some((msg_id, seq, content));
    }

    // Decrypt history for the model context.
    let rows = messages::list(pool, conv_id)
        .await
        .map_err(|e| internal(e, "list messages"))?;
    let mut history = vec![ChatMessage {
        role: "system".into(),
        content: SYSTEM_PROMPT.into(),
    }];
    let mut first_user_content: Option<String> = None;
    let mut user_turns = 0usize;
    for m in &rows {
        let content = envelope::decrypt_message(&dek, conv_id, m.id, m.seq, &m.role, &m.ciphertext)
            .map_err(|e| internal(e, "message decrypt"))?;
        if m.role == "user" {
            user_turns += 1;
            if first_user_content.is_none() {
                first_user_content = Some(content.clone());
            }
        }
        history.push(ChatMessage {
            role: m.role.clone(),
            content,
        });
    }
    if user_turns == 0 {
        return Err(err(Status::BadRequest, "no_user_turn"));
    }
    let needs_auto_title = conv.enc_title.is_none() && user_turns == 1;

    let assistant_msg_id = Uuid::new_v4();
    let assistant_seq = messages::next_seq(pool, conv_id)
        .await
        .map_err(|e| internal(e, "next seq"))?;

    let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
    let meta = json!({
        "conversation_id": conv_id,
        "user_message_id": user_msg.as_ref().map(|(id, _, _)| *id),
        "user_message_seq": user_msg.as_ref().map(|(_, s, _)| *s),
        "assistant_message_id": assistant_msg_id,
        "assistant_message_seq": assistant_seq,
        "model_id": model_id,
    });
    let _ = tx.send(StreamEvent::Meta(meta)).await;

    // Drive the upstream + persistence in a detached task: if the client
    // disconnects (stop button / closed tab), the partial message is still
    // encrypted and stored, and metering still settles.
    let gen = GenerationCtx {
        pool: pool.inner().clone(),
        cfg: cfg.inner().clone(),
        or_client: or_client.inner().clone(),
        api_key,
        model_id,
        history,
        dek,
        kek,
        user_ref_hash: user.user_ref_hash.clone(),
        conv_id,
        assistant_msg_id,
        assistant_seq,
        needs_auto_title,
        first_user_content,
        max_tokens: cfg.max_tokens_cap,
    };
    tokio::spawn(run_generation(gen, tx));

    Ok(EventStream! {
        while let Some(ev) = rx.recv().await {
            match ev {
                StreamEvent::Meta(v) => yield Event::json(&v).event("meta"),
                StreamEvent::Delta(text) => yield Event::json(&json!({"d": text})).event("delta"),
                StreamEvent::Done(usage) => {
                    yield Event::json(&json!({
                        "prompt_tokens": usage.prompt_tokens,
                        "completion_tokens": usage.completion_tokens,
                    })).event("done");
                }
                StreamEvent::Error(msg) => yield Event::json(&json!({"error": msg})).event("error"),
            }
        }
    })
}

struct GenerationCtx {
    pool: PgPool,
    cfg: Config,
    or_client: OpenRouterClient,
    api_key: String,
    model_id: String,
    history: Vec<ChatMessage>,
    dek: [u8; 32],
    kek: [u8; 32],
    user_ref_hash: String,
    conv_id: Uuid,
    assistant_msg_id: Uuid,
    assistant_seq: i64,
    needs_auto_title: bool,
    first_user_content: Option<String>,
    max_tokens: i64,
}

async fn run_generation(ctx: GenerationCtx, tx: mpsc::Sender<StreamEvent>) {
    let result = ctx
        .or_client
        .stream_chat(
            &ctx.api_key,
            &ctx.model_id,
            &ctx.history,
            ctx.max_tokens,
            &tx,
        )
        .await;
    let (text, usage, _client_gone) = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(conversation_id = %ctx.conv_id, "generation failed: {e}");
            return;
        }
    };
    if text.is_empty() {
        return;
    }

    // Encrypt-and-store the completed (or partial-on-stop) message.
    let enc_usage = usage.as_ref().and_then(|u| {
        let json = json!({
            "prompt_tokens": u.prompt_tokens,
            "completion_tokens": u.completion_tokens,
            "cost_micro_usd": u.cost_micro_usd,
        })
        .to_string();
        envelope::encrypt_usage_meta(&ctx.dek, ctx.conv_id, ctx.assistant_msg_id, &json).ok()
    });
    let ct = match envelope::encrypt_message(
        &ctx.dek,
        ctx.conv_id,
        ctx.assistant_msg_id,
        ctx.assistant_seq,
        "assistant",
        &text,
    ) {
        Ok(ct) => ct,
        Err(e) => {
            tracing::warn!("assistant message encrypt failed: {e}");
            return;
        }
    };
    if let Err(e) = messages::append(
        &ctx.pool,
        ctx.assistant_msg_id,
        ctx.conv_id,
        ctx.assistant_seq,
        "assistant",
        &ct,
        enc_usage.as_deref(),
    )
    .await
    {
        tracing::warn!("assistant message store failed: {e}");
        return;
    }
    let _ = conversations::touch(&ctx.pool, ctx.conv_id).await;

    // Auto-title after the first exchange.
    if ctx.needs_auto_title {
        if let Some(first) = &ctx.first_user_content {
            let title: String = first
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(AUTO_TITLE_MAX_CHARS)
                .collect();
            if !title.is_empty() {
                if let Ok(enc) = envelope::encrypt_title(&ctx.dek, ctx.conv_id, &title) {
                    let _ = conversations::set_title_unchecked(
                        &ctx.pool,
                        ctx.conv_id,
                        &ctx.user_ref_hash,
                        &enc,
                    )
                    .await;
                }
            }
        }
    }

    // Settle after, never mid-stream, never fatally (section 8.3).
    if let Some(u) = usage {
        let tokens = u.prompt_tokens + u.completion_tokens;
        if let Err(e) = metering::settle(
            &ctx.pool,
            &ctx.cfg,
            &ctx.kek,
            &ctx.user_ref_hash,
            u.cost_micro_usd,
            tokens,
        )
        .await
        {
            tracing::warn!("meter settle failed (absorbed): {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Export + account deletion (section 4.1)

#[get("/api/export")]
pub async fn export(
    user: ChatUser,
    pool: &State<PgPool>,
    key_source: &State<KeySource>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kek = user_kek(key_source, &user.user_ref)
        .await
        .map_err(|e| internal(e, "kek derive"))?;
    let convs = conversations::list_for_user(pool, &user.user_ref_hash)
        .await
        .map_err(|e| internal(e, "list conversations"))?;
    let mut out = Vec::with_capacity(convs.len());
    for conv in convs {
        let dek = envelope::unwrap_dek(&kek, &conv.wrapped_dek, &user.user_ref_hash, conv.id)
            .map_err(|e| internal(e, "dek unwrap"))?;
        let title = conv
            .enc_title
            .as_ref()
            .and_then(|enc| envelope::decrypt_title(&dek, conv.id, enc).ok());
        let msgs = messages::list(pool, conv.id)
            .await
            .map_err(|e| internal(e, "list messages"))?;
        let mut plain = Vec::with_capacity(msgs.len());
        for m in msgs {
            let content =
                envelope::decrypt_message(&dek, conv.id, m.id, m.seq, &m.role, &m.ciphertext)
                    .map_err(|e| internal(e, "message decrypt"))?;
            plain.push(json!({
                "seq": m.seq,
                "role": m.role,
                "content": content,
                "created_at": m.created_at
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            }));
        }
        out.push(json!({
            "id": conv.id,
            "title": title,
            "model_id": conv.model_id,
            "messages": plain,
        }));
    }
    Ok(Json(json!({
        "exported_at": time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        "conversations": out,
    })))
}

#[post("/api/account/delete")]
pub async fn delete_account(
    user: ChatUser,
    _csrf: Csrf,
    pool: &State<PgPool>,
    cookies: &rocket::http::CookieJar<'_>,
) -> Result<Json<serde_json::Value>, ApiError> {
    users::revoke_session(pool, &user.token_hash)
        .await
        .map_err(|e| internal(e, "revoke session"))?;
    users::delete(pool, &user.user_ref_hash)
        .await
        .map_err(|e| internal(e, "delete user"))?;
    cookies.remove(
        rocket::http::Cookie::build((session::SESSION_COOKIE, ""))
            .path("/")
            .build(),
    );
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------------------
// Trust surface: breaker/status info for the privacy panel.

#[get("/api/status")]
pub async fn status(
    pool: &State<PgPool>,
    cfg: &State<Config>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mode = settings::breaker_mode(pool)
        .await
        .map_err(|e| internal(e, "breaker mode"))?;
    let today = metering::today_utc();
    let spent = meters::day_spend(pool, &today)
        .await
        .map_err(|e| internal(e, "day spend"))?;
    let defaults = settings::Caps {
        daily_spend_cap_micro_usd: cfg.daily_spend_cap_micro_usd,
        anon_daily_token_budget: cfg.anon_daily_token_budget,
    };
    let caps = settings::caps(pool, &defaults)
        .await
        .map_err(|e| internal(e, "caps"))?;
    Ok(Json(json!({
        "breaker_mode": mode,
        "day_spend_micro_usd": spent,
        "daily_cap_micro_usd": caps.daily_spend_cap_micro_usd,
    })))
}

#[get("/health")]
pub fn health() -> &'static str {
    "ok"
}
