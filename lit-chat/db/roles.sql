-- Lit Chat DB roles (plans/tee-chat-app.md section 4.4).
-- Run ONCE as the database owner/superuser after the first deploy has run
-- migrations. Not a sqlx migration on purpose: role management needs
-- privileges the app user doesn't have.
--
-- Set real passwords (or use Railway-managed credentials) before running:
--   \set app_password  '...'
--   \set admin_password '...'

-- Consumer service role: full data-plane access, read-only on the admin
-- plane's provider_keys/settings (it consumes the active key + caps), and
-- NO access to the roster or audit chain beyond what it writes itself.
CREATE ROLE lit_chat_app LOGIN PASSWORD :'app_password';
GRANT SELECT, INSERT, UPDATE, DELETE ON
  chat_users, conversations, messages, sessions_revoked, magic_links,
  user_meters, spend_days, model_catalog
TO lit_chat_app;
GRANT SELECT ON provider_keys, settings TO lit_chat_app;
-- Bootstrap import (first boot) writes provider_keys once:
GRANT INSERT ON provider_keys TO lit_chat_app;
-- sqlx migrations table:
GRANT SELECT, INSERT, UPDATE, DELETE ON _sqlx_migrations TO lit_chat_app;

-- Admin service role: the admin plane ONLY. SELECT on messages/conversations
-- is deliberately NOT granted — a fully compromised admin plane can spend
-- money and break inference; it cannot read chats.
CREATE ROLE lit_chat_admin LOGIN PASSWORD :'admin_password';
GRANT SELECT, INSERT, UPDATE ON provider_keys TO lit_chat_admin;
GRANT SELECT, INSERT, DELETE ON admins TO lit_chat_admin;
GRANT SELECT, INSERT ON admin_credentials, admin_audit_log TO lit_chat_admin;
GRANT SELECT, INSERT, UPDATE ON settings TO lit_chat_admin;
GRANT SELECT, UPDATE ON model_catalog TO lit_chat_admin;
GRANT SELECT ON spend_days TO lit_chat_admin;
-- Admin login shares the session-revocation + magic-link replay tables:
GRANT SELECT, INSERT, UPDATE ON sessions_revoked, magic_links TO lit_chat_admin;
-- Audit log id sequence:
GRANT USAGE, SELECT ON SEQUENCE admin_audit_log_id_seq TO lit_chat_admin;
