-- lit-approvals shared store (Neon). Run once per environment.
-- This store is UNTRUSTED: it provides shared state that survives horizontal
-- scaling + ping-pong deploys, but integrity comes from the signed attestation
-- (verified in-TEE), never from DB access control. See README "Threat model".

create table if not exists lit_approvals (
  approval_id    text primary key,
  approver       text not null,
  assurance      text not null check (assurance in ('L1','L2')),
  request_hash   text not null default '',   -- operation binding; empty = L1
  status         text not null default 'pending',  -- pending|consumed|denied|expired
  otp_hmac       text not null default '',   -- HMAC(otpKey, id:otp); otpKey is TEE-held
  clicked        boolean not null default false,
  submitted_otp  text,                       -- written by the (untrusted) approval page
  attestation    text,                       -- signed envelope, written once on consume
  created_at_ms  bigint not null,
  expires_at_ms  bigint not null
);

-- Optional: reap expired/consumed rows on a schedule.
create index if not exists lit_approvals_status_idx on lit_approvals (status);
create index if not exists lit_approvals_expires_idx on lit_approvals (expires_at_ms);
