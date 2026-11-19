// Runs the SDK's attestation verifier against a live deployment.
//
//   node lit-static/sdk/scripts/verify-live.mjs <base-url> <staging|production>
//        [--require-binding | --no-binding] [--expect-instance <instance id>] [--expect-platform <platform>]
//
// Used as a deployment gate: a release only proceeds if the verifier customers
// run accepts the box that was just deployed. Two modes:
//
//   Default, for an instance's own Phala URL. That URL has no built-in policy,
//   so the policy is named explicitly, and TLS binding is off because the
//   gateway's certificate, not the app's, serves it. An Intel TDX box is
//   reported and skipped (exit 0): this verifier covers AMD SEV-SNP, and TDX
//   deployments have their own gate.
//
//   --require-binding, for a public hostname that must be AMD with its TLS
//   bound to the attested VM. Nothing is skipped: any other platform, and any
//   result other than a verified binding, fails.
//
//   --no-binding, for a public hostname deliberately deployed without TLS
//   evidence (ingress 1.x). The attestation is still verified; the binding the
//   hostname's built-in policy would require is not attempted.
//
// --expect-platform (for example sev-snp) fails on any other platform instead
// of skipping it: a hostname that is meant to be AMD must not pass on Intel.
//
// --expect-instance fails unless the VM that answered is that instance, so a
// check made while two replicas overlap cannot be satisfied by the old one.
import { verifyAttestation, AttestationError, ATTESTED_ORIGINS, LIT_PRODUCTION_POLICY, LIT_STAGING_POLICY } from '../../attestation.js';

const args = process.argv.slice(2);
const flag = (name) => {
  const at = args.indexOf(name);
  if (at === -1) return false;
  args.splice(at, 1);
  return true;
};
const option = (name) => {
  const at = args.indexOf(name);
  if (at === -1) return null;
  const [, value] = args.splice(at, 2);
  if (!value) usage();
  return value;
};
function usage() {
  console.error('usage: verify-live.mjs <base-url> <staging|production> [--require-binding | --no-binding] [--expect-instance <id>] [--expect-platform <platform>]');
  process.exit(2);
}
const requireBinding = flag('--require-binding');
const noBinding = flag('--no-binding');
if (requireBinding && noBinding) usage();
const expectInstance = option('--expect-instance')?.toLowerCase().replace(/^0x/, '') ?? null;
const expectPlatform = option('--expect-platform') ?? (requireBinding ? 'sev-snp' : null);
const [baseUrl, policyName] = args;
const policies = { staging: LIT_STAGING_POLICY, production: LIT_PRODUCTION_POLICY };
const policy = policies[policyName];
if (!baseUrl || !policy || args.length !== 2) usage();

let origin;
try {
  origin = new URL(baseUrl).origin;
} catch {
  console.error(`invalid base URL: ${baseUrl}`);
  process.exit(2);
}
const rpcUrls = process.env.BASE_RPC_URL ? [process.env.BASE_RPC_URL, ...policy.rpcUrls] : policy.rpcUrls;
const failed = (message) => {
  console.error(`::error::SDK attestation verification failed for ${origin}: ${message}`);
  process.exit(1);
};

let result;
try {
  result = await verifyAttestation(origin, {
    appId: policy.appId,
    kmsContract: policy.kmsContract,
    requireUpToDateTcb: policy.requireUpToDateTcb,
    rpcUrls,
    tlsBinding: requireBinding ? 'strict' : !noBinding && ATTESTED_ORIGINS[origin] ? policy.tlsBinding : 'off',
  });
} catch (error) {
  if (!expectPlatform && error instanceof AttestationError && error.code === 'unsupported_platform' && /\btdx\b/.test(error.message)) {
    console.log(`::notice::${origin} is an Intel TDX deployment; the SDK verifier covers AMD SEV-SNP. Skipped.`);
    process.exit(0);
  }
  failed(`[${error.code ?? 'error'}] ${error.message}`);
}

console.log(JSON.stringify(result, null, 2));
if (expectPlatform && result.platform !== expectPlatform) {
  failed(`expected a ${expectPlatform} deployment, got ${result.platform}`);
}
if (requireBinding && (result.tlsBinding !== 'verified' || !result.tlsKeySha256)) {
  failed(`expected verified TLS binding, got ${result.tlsBinding}`);
}
if (expectInstance && result.instanceId !== expectInstance) {
  failed(`answered by instance ${result.instanceId}, expected ${expectInstance}`);
}
console.log(`SDK attestation verified: ${origin} is ${result.platform}, instance ${result.instanceId}, TCB ${result.tcbStatus}, TLS binding ${result.tlsBinding}.`);
