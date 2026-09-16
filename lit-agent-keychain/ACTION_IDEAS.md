# Action ideas: where "use inside Lit" adds a capability the provider lacks

Brainstorm captured 2026-09-15. Not a roadmap. A catalog action is worth building
when three things line up:

1. **The provider's credential is all-or-nothing.** A Stripe secret key, a Supabase
   service role key, an OAuth refresh token, a registrar API key. The provider
   offers no way to say "refunds under $50 only" or "this one table only".
2. **The call has real side effects or the response leaks data.** Money moves, a
   message goes out, a record is deleted, or the reply contains rows an agent
   should never hold.
3. **No extra infrastructure for the owner.** The credential is one they already
   have. Nothing to host, nothing to keep online.

The action runs in the enclave with the decrypted credential, a pinned host list, a
validated input shape and a projected output shape (see `actions/secret-common.ts`
and the [library README](https://github.com/LIT-Protocol/agent-keychain-library)).
That lets us bolt a policy onto any API that the API itself does not support.
The examples below are grouped by the policy pattern they demonstrate.

## Patterns the enclave can enforce that providers cannot

| Pattern             | Enforced how                                                                  | Example                                  |
| ------------------- | ----------------------------------------------------------------------------- | ---------------------------------------- |
| Amount cap          | `input.amount <= cap` before the call; cap from manifest or credential suffix | Stripe refund ≤ $50                      |
| Recipient allowlist | allowlist stored alongside the credential; input must match                   | Wire only to known counterparties        |
| Verb subsetting     | action exposes one or two endpoints of a broad API                            | Cloudflare: update one DNS record        |
| Output projection   | return shape drops bodies, rows, PII                                          | Gmail: subjects and senders, no bodies   |
| Precondition check  | a read call gates the write call (`maxRequests: 2`)                           | Merge PR only if CI is green             |
| Template pinning    | action fixes the message/system prompt; agent fills slots                     | LLM key with a pinned system prompt      |
| Time window         | enclave clock; refuse outside hours                                           | No SMS between 22:00 and 08:00           |
| Downscoping         | root credential in, short-lived narrow token out                              | AWS STS AssumeRole                       |
| Owner approval      | owner-signed hash of the exact request, verified in-enclave                   | "Ask me above $500" (needs harness work) |
| Counters and quotas | needs trusted state; not possible in a stateless action today                 | $1,000 per day total                     |

The first eight work today with a manifest and an `action.ts`. Approval needs a
signature-verification helper in the library or an approval receipt in the
protocol. Counters need state the enclave trusts; the Chipotle spending-rules
work is the closest existing primitive.

## Ideas by area

### Money movement

- **Stripe capped refunds.** Refund a charge up to a per-call cap, only to the
  original payment method. Restricted keys scope by resource, never by amount.
- **Stripe sell-but-don't-move.** Create Checkout links, payment links, invoices
  to existing customers, coupons up to N percent. Never payouts, never transfers.
- **Bank transfers with an allowlist.** Mercury, Brex, Ramp, Wise: transfer up to
  a cap, only to counterparties the owner listed when storing the credential.
- **Exchange trading, no withdrawals.** Coinbase, Kraken, Binance: trade an
  allowlisted set of pairs under a notional cap. The withdraw endpoint is
  simply not reachable.
- **Brokerage orders.** Alpaca, IBKR: ticker allowlist, notional cap, market
  hours only, live/paper guard.
- **Shopify and Stripe-like storefronts.** Fulfill orders, issue capped refunds,
  never edit products or payout settings.
- **Payroll and HR.** Gusto, Rippling: read headcount and PTO balances. Any
  write is approval-gated.

### Sending as the owner, without a machine to keep online

- **Gmail send via OAuth refresh token.** The refresh token is a permanent key to
  the whole mailbox. The action exchanges it for an access token and sends one
  message, from the owner's real address. Recipient allowlist, subject prefix,
  no reads. This is the frictionless answer to "let my agent message people as
  me". (iMessage needs an always-on Mac running BlueBubbles; the friction is
  the machine, not the secret. Parked.)
- **Gmail read, projected.** Return subject, sender, date and label for the
  last N threads. Never bodies or attachments. "Inbox triage without inbox
  access."
- **Twilio SMS and WhatsApp.** From the owner's Twilio number. Allowlist, or
  reply-only to numbers that messaged first (one read call gates the send),
  quiet hours, template pinning.
- **Transactional email providers.** SendGrid, Resend, Postmark: from-address
  pinned, template-only sends, no list or contact exports.
- **Slack, Discord, Telegram, channel-pinned.** The shipped Slack action allows
  any channel; a variant that pins one channel is a policy Slack scopes cannot
  express. Telegram: reply only to allowlisted chat ids.
- **Social posting.** X, LinkedIn, Bluesky: post under a length cap, no DMs, no
  deletes.
- **Calendar and booking.** Create events in one named calendar; Cal.com
  bookings; DocuSign or PandaDoc: send one named template to an allowlisted
  signer.
- **Support desks.** Zendesk, Intercom: reply to a ticket or save a draft.
  Never close, merge or delete. Never list users.

### God-mode infrastructure keys, narrowed to verbs

- **Supabase service role and Neon or PlanetScale admin keys.** The action runs
  only named parameterized queries, caps returned rows, and masks PII columns.
  Every Supabase app has this key and every agent wants it. Probably the
  single highest-value action for agent builders.
- **Deploy platforms.** Vercel, Railway, Fly, Render: redeploy or roll back one
  project, read logs, return env var names but never values.
- **Cloudflare.** Update one DNS record, purge cache, toggle Under Attack mode.
  API tokens are zone-scoped, never record-scoped.
- **Registrars.** Namecheap, Porkbun, GoDaddy: set one TXT record (ACME) and
  nothing else. Registrar APIs are all-or-nothing and a compromised one loses
  the domain.
- **AWS and GCP.** Sign a fixed allowlist of API calls inside the action, with
  caps IAM cannot express (`desiredCount <= 4`, only during business hours),
  or downscope: return a 15-minute STS token with a narrow session policy.
- **Kubernetes and Docker registries.** Read pods and logs, restart one
  deployment, pull but never push.
- **Package registries.** npm, PyPI, crates.io: publish only a named package,
  only under a prerelease dist-tag, only if the version is greater than the
  current one (read call gates publish). Directly relevant to supply-chain
  incidents where an agent held a publish token.
- **Code hosting.** GitHub, GitLab, Bitbucket: comment, open PRs, read files,
  approve PRs the agent did not author, merge only when checks are green.
  Never push to a protected branch or change settings.
- **Identity providers.** Auth0, Clerk, WorkOS management keys: look up a
  user, resend verification, reset MFA with approval. Never delete a tenant.
- **Observability.** Datadog, Grafana, Sentry: query and read; mute an alert
  for at most N minutes; never delete dashboards or rotate keys.

### Data access with projection

- **Plaid.** Return balances only; the transactions endpoint is unreachable.
- **CRMs.** HubSpot, Salesforce, Attio: create a contact or note, look up one
  record by id. No list or export endpoints, so the credential cannot be used
  to dump the customer base.
- **Notion, Airtable, Google Sheets.** Read one database or append one row.
  Notion tokens are workspace-wide for every shared page.
- **Analytics.** PostHog, Mixpanel, GA: aggregate queries only, row cap.

### Keys as secrets

- **Bring-your-own Ethereum or Solana key.** A key with history and existing
  allowances. The action signs only ERC-20 transfers to an allowlist under a
  cap, or only a specific contract call. Chipotle covers fresh PKPs; this
  covers keys people already have.
- **TOTP seeds.** Return the current six-digit code; the seed never leaves.
  The agent can pass 2FA but cannot clone the authenticator. Pair with
  approval for high-value logins.
- **JWT and webhook signing keys.** Mint short-lived tokens with fixed claims,
  or HMAC-sign a payload of a fixed shape. The agent cannot forge anything
  outside the template.
- **SSH deploy keys and code-signing keys.** Sign one artifact digest; the
  action never returns the key.
- **LLM provider keys with a pinned prompt.** Model allowlist, `max_tokens`
  cap, and a system prompt fixed in the manifest that the agent cannot
  override. Anthropic and OpenAI keys cannot be restricted per model or per
  prompt.
- **Home Assistant long-lived tokens.** Toggle allowlisted entities. Locks and
  garage doors are approval-gated. A consumer-facing example of the pattern.

## Horizontal features that multiply everything above

- **Owner approval receipts.** Nearly every item has a "fine under X, ask me
  above X" version. Two viable shapes: an approval receipt kind in the
  protocol verified by the harness, or a `recoverAddress` helper in the
  library so an action can verify an owner-signed hash of its own input.
- **Counters and quotas.** Daily totals, message counts per recipient, rate
  limits. Stateless actions cannot do this; the enclave needs state it
  trusts. Chipotle's spending rules are the closest primitive to reuse.
- **Many agents, one credential.** Each agent has its own key, expiry, grant
  and audit trail. Revoke one without rotating the provider credential.
- **Owner-pinned hosts.** `allowedHosts` is fixed per manifest, so self-hosted
  targets (a personal Supabase project domain, a BlueBubbles tunnel) need
  either a personal community-tier action or a harness mode that binds a
  host from the stored credential at secret creation.

## Constraints to design around

- Actions are stateless and one-shot. Per-call limits yes; running totals no.
- `allowedHosts` is 1 to 8 exact hostnames, HTTPS only, no redirects.
- Output is at most 16 KiB and must match the declared shape.
- `action.ts` may import only `lib.ts`. No crypto, timers, globals or Lit
  runtime, so signature checks need a library helper first.
- Every throw is `access_denied`; upstream errors never reach the agent.

## Suggested first three

Highest usefulness per unit of work, all using credentials people already hold:

1. Gmail send via refresh token (send as me, no machine).
2. Supabase service-role parameterized queries.
3. Stripe capped refunds.
