-- The browser LITKEY payment flow now verifies the submitted transaction hash
-- directly with eth_getTransactionReceipt. The background WSS/reconciliation
-- listener is no longer started, so its checkpoint table is obsolete.
--
-- This is intentionally a new migration instead of editing
-- 20260522000001_litkey_listener.sql: that migration has already been applied in
-- deployed databases and sqlx verifies applied migration checksums at startup.
DROP TABLE IF EXISTS chain_checkpoint;
