-- CPL-375: close the LITKEY double-credit window after a crash.
--
-- Before this change `handle_confirmed_litkey_payment` wrote the Stripe
-- balance_transaction FIRST and inserted the `litkey_payments` row LAST.
-- A crash between those two steps left the credit at Stripe with no DB
-- record; because Stripe drops a given Idempotency-Key after ~24h, a
-- re-claim more than a day later re-issued the credit and double-credited
-- the user.
--
-- The fix inserts the row BEFORE the Stripe call as the idempotency guard,
-- then fills in `stripe_balance_transaction_id` once the credit lands. That
-- makes a `credited` row legal in a "partial" state (credit intended, Stripe
-- id not yet recorded), which the original status-fields CHECK forbade.
-- A background reconciler (mirroring the auto_topup one) completes any
-- partial row within Stripe's idempotency window using the same
-- `litkey:{chain}:{tx}:{log_index}` key, so the retry dedupes instead of
-- double-crediting.
--
-- This migration relaxes the credited branch of the CHECK to drop the
-- `stripe_balance_transaction_id IS NOT NULL` requirement, and adds a
-- partial index the reconciler uses to find rows still awaiting their
-- balance_transactions write.

ALTER TABLE litkey_payments
    DROP CONSTRAINT litkey_payments_status_fields_check;

ALTER TABLE litkey_payments
    ADD CONSTRAINT litkey_payments_status_fields_check CHECK (
        (status = 'credited' AND usd_wei_per_litkey IS NOT NULL AND cents_credited > 0 AND stripe_customer_id IS NOT NULL)
        OR (status <> 'credited' AND stripe_balance_transaction_id IS NULL)
    );

-- Reconciler lookup: credited rows whose balance_transactions write has not
-- landed yet (crash or Stripe error between INSERT and the credit). Partial
-- so the index stays trivially small under normal operation.
CREATE INDEX litkey_payments_pending_balance_tx_idx
    ON litkey_payments (credited_at)
    WHERE status = 'credited' AND stripe_balance_transaction_id IS NULL;
