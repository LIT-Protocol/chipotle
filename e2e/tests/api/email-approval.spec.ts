import { test, expect } from '@playwright/test';
import { LitApiClient } from '../../fixtures/api-client';

/**
 * M3 gate (plan D6): two-phase email approval.
 *
 *  phase 1 — an action calls requestEmailApproval (L2) and exits
 *  human    — opens the approval link, fails a wrong OTP, approves with the
 *             right one (the OTP travels via the requesting app, not email)
 *  phase 2 — a later action calls checkEmailApproval; the RUNTIME verifies
 *            the attestation in-TEE before reporting approved
 *
 * Requirements on the target env (dev/local only — never prod):
 *  - lit-api-server runs with LIT_APPROVAL_EXPOSE_LINK=true so the test can
 *    "click" the link without an inbox (otherwise this spec skips)
 *  - the lit-actions runtime has LIT_APPROVAL_ATTESTATION_PUBKEY pinned to
 *    the server's /approvals_pubkey (otherwise phase 2 fails closed, which
 *    the spec reports as a clear configuration failure)
 */

const APPROVER = 'approver-e2e@example.com';

function actionRequestApproval(summary: string): string {
  return `
async function main() {
  const r = await Lit.Actions.requestEmailApproval({
    to: ${JSON.stringify(APPROVER)},
    summary: ${JSON.stringify(summary)},
    assurance: 'L2',
    ttlSec: 300,
  });
  return { approvalId: r.approvalId, otp: r.otp ?? null, approvalUrl: r.approvalUrl ?? null };
}
`;
}

function actionCheckApproval(approvalId: string): string {
  return `
async function main() {
  try {
    const r = await Lit.Actions.checkEmailApproval({ approvalId: ${JSON.stringify(approvalId)} });
    return { ok: true, approved: r.approved, status: r.status, approver: r.approver ?? null, hasAttestation: !!r.attestation };
  } catch (e) {
    return { ok: false, error: String((e && e.message) || e).slice(0, 300) };
  }
}
`;
}

async function postDecision(url: string, fields: Record<string, string>): Promise<string> {
  const [pageUrl, query] = url.split('?');
  const token = new URLSearchParams(query).get('t')!;
  const res = await fetch(pageUrl!, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ t: token, ...fields }).toString(),
  });
  return res.text();
}

test.describe('email approval — M3 two-phase flow', () => {
  test('request (L2) → wrong OTP refused → approve → in-TEE-verified check; deny path; tamper-resistant', async () => {
    test.setTimeout(180_000);
    const apiClient = new LitApiClient();
    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-approval-${stamp}`,
      account_description: 'email-approval M3 gate',
    });
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-approval-${stamp}-usage`,
      description: 'email-approval M3 gate',
      execute_in_groups: [0],
    });

    // ---- phase 1: request the approval from inside an action
    const phase1 = await apiClient.litAction(usageApiKey, {
      code: actionRequestApproval(`Sweep 0.1 BTC to cold storage (e2e ${stamp})`),
    });
    expect(phase1.has_error, `phase 1 errored; logs: ${phase1.logs ?? '<none>'}`).toBe(false);
    const { approvalId, otp, approvalUrl } = phase1.response as {
      approvalId: string;
      otp: string | null;
      approvalUrl: string | null;
    };
    expect(approvalId).toMatch(/^apr_[0-9a-f]{32}$/);
    expect(otp, 'L2 must issue an OTP to the requesting app').toMatch(/^\d{6}$/);
    test.skip(!approvalUrl, 'server runs without LIT_APPROVAL_EXPOSE_LINK — enable it on dev to run this gate');

    // ---- pending before any decision
    const pending = await apiClient.litAction(usageApiKey, { code: actionCheckApproval(approvalId) });
    expect(pending.has_error).toBe(false);
    expect(pending.response).toMatchObject({ ok: true, approved: false, status: 'pending' });

    // ---- the approval page renders the summary and the OTP field
    const pageRes = await fetch(approvalUrl!);
    expect(pageRes.status).toBe(200);
    const pageHtml = await pageRes.text();
    expect(pageHtml).toContain('Sweep 0.1 BTC');
    expect(pageHtml).toContain('one-time-code');

    // ---- wrong OTP is refused and the approval stays pending (retryable)
    const wrong = await postDecision(approvalUrl!, { otp: '000000', decision: 'approve' });
    expect(wrong).toContain('incorrect or missing code');

    // ---- correct OTP approves (single-use)
    const approved = await postDecision(approvalUrl!, { otp: otp!, decision: 'approve' });
    expect(approved).toContain('Approved');
    const replay = await postDecision(approvalUrl!, { otp: otp!, decision: 'approve' });
    expect(replay).toContain('already approved');

    // ---- phase 2: the action checks the approval; the runtime verifies in-TEE
    const phase2 = await apiClient.litAction(usageApiKey, { code: actionCheckApproval(approvalId) });
    expect(phase2.has_error).toBe(false);
    const verdict = phase2.response as {
      ok: boolean;
      approved?: boolean;
      status?: string;
      approver?: string;
      hasAttestation?: boolean;
      error?: string;
    };
    if (!verdict.ok && /LIT_APPROVAL_ATTESTATION_PUBKEY/.test(verdict.error ?? '')) {
      throw new Error(
        'runtime is missing LIT_APPROVAL_ATTESTATION_PUBKEY — pin it to GET /approvals_pubkey of this env ' +
          `(in-TEE verification fails closed by design). Raw: ${verdict.error}`,
      );
    }
    expect(verdict).toMatchObject({
      ok: true,
      approved: true,
      status: 'approved',
      approver: APPROVER,
      hasAttestation: true,
    });

    // ---- deny path: a fresh approval, denied at the page, reports denied (no attestation)
    const denyPhase1 = await apiClient.litAction(usageApiKey, {
      code: actionRequestApproval(`Deny-path probe (e2e ${stamp})`),
    });
    expect(denyPhase1.has_error).toBe(false);
    const deny = denyPhase1.response as { approvalId: string; approvalUrl: string | null };
    const denied = await postDecision(deny.approvalUrl!, { decision: 'deny' });
    expect(denied).toContain('Denied');
    const denyCheck = await apiClient.litAction(usageApiKey, { code: actionCheckApproval(deny.approvalId) });
    expect(denyCheck.response).toMatchObject({ ok: true, approved: false, status: 'denied' });
  });

  test('sendEmail op accepts a plain notification within quota', async () => {
    const apiClient = new LitApiClient();
    const stamp = Date.now();
    const account = await apiClient.newAccount({
      account_name: `e2e-sendmail-${stamp}`,
      account_description: 'sendEmail smoke',
    });
    const { usage_api_key: usageApiKey } = await apiClient.addUsageApiKey(account.api_key, {
      name: `e2e-sendmail-${stamp}-usage`,
      description: 'sendEmail smoke',
      execute_in_groups: [0],
    });
    const result = await apiClient.litAction(usageApiKey, {
      code: `
async function main() {
  const r = await Lit.Actions.sendEmail({
    to: ${JSON.stringify(APPROVER)},
    subject: 'venue drift summary',
    text: 'all green',
  });
  return r;
}
`,
    });
    expect(result.has_error, `sendEmail errored; logs: ${result.logs ?? '<none>'}`).toBe(false);
    expect(result.response).toMatchObject({ accepted: true });
  });
});
