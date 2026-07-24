-- Flip Uneven Labs to auto-send: the billing job now finalizes + sends the
-- monthly invoice itself instead of leaving a draft for manual review (the
-- July 2026 cycle validated the draft flow end-to-end). notify_email gets an
-- FYI breakdown instead of a review request. Anomalous cycles (0 consumed
-- units) are still held as drafts for human review.
UPDATE enterprise_accounts
SET auto_send = true, updated_at = now()
WHERE payer_customer_id = 'cus_UXvHcFlfhR6rc5';
