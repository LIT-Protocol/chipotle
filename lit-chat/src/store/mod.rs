//! Data access. All queries are runtime `sqlx::query`/`query_as` with binds
//! (house pattern: no compile-time DATABASE_URL). Tenancy is enforced in the
//! SQL (`AND user_ref_hash = $n`), not in a filter step.

pub mod admin;
pub mod conversations;
pub mod messages;
pub mod meters;
pub mod provider_keys;
pub mod settings;
pub mod users;
