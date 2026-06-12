-- Codex P1 (Phase 4): enumerate the allowed `disabled_reason` values.
-- Without this CHECK, a buggy code path could write any string and the
-- dashboard's branching logic (which only recognises the four below)
-- would silently fall through. The application code already writes
-- only these values; the constraint is a durable guard against future
-- typos / additions that don't update the dashboard.
ALTER TABLE auto_topup_config
    ADD CONSTRAINT auto_topup_disabled_reason_valid
    CHECK (
        disabled_reason IS NULL
        OR disabled_reason IN ('manual', 'failures', 'card_invalid', 'requires_action')
    );
