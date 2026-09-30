-- Durable marker for the auto-send finalize step. Set immediately after Stripe
-- confirms finalize+send, BEFORE the FYI email and the status='sent' write, so
-- a crash between them resumes at the email step instead of replaying the
-- Stripe calls (whose idempotency keys expire after ~24h).
ALTER TABLE enterprise_invoices ADD COLUMN finalized_at TIMESTAMPTZ;
