pub mod agent;
pub mod rate_limit;
pub mod routes;
pub mod session;
pub mod token;
pub mod user;

pub use user::User;

pub const SESSION_COOKIE_NAME: &str = "lit_triggers_session";
pub const MAGIC_LINK_TTL_SECONDS: i64 = 15 * 60;
pub const SESSION_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
