/**
 * Type declarations for `attestation.js` (`@lit-protocol/core-sdk/attestation`).
 * Written by hand: keep them in step with the JavaScript. `sdk/scripts/types.test.mjs`
 * fails when an export is added to one file and not the other.
 */

/** Stable `code` values of an `AttestationError`. */
export type AttestationErrorCode =
  | 'invalid_argument'
  | 'no_policy'
  | 'endpoint_unavailable'
  | 'endpoint_busy'
  | 'malformed_evidence'
  | 'unsupported_platform'
  | 'collateral_unavailable'
  | 'untrusted_hardware'
  | 'stale_evidence'
  | 'policy_violation'
  | 'measurement_mismatch'
  | 'identity_mismatch'
  | 'not_governed'
  | 'governance_unavailable'
  | 'tls_binding_failed'
  | 'tls_binding_unavailable';

/** Thrown for every verification failure. `code` is stable; `message` is for people. */
export class AttestationError extends Error {
  constructor(code: AttestationErrorCode, message: string, options?: { cause?: unknown });
  name: 'AttestationError';
  code: AttestationErrorCode;
}

/** How the TLS certificate of a connection is tied to the attested VM. */
export type TlsBindingMode = 'auto' | 'require' | 'strict' | 'off';

/** Firmware versions of an SEV-SNP platform. `fmc` is present on Turin only. */
export interface SnpTcbVersion {
  bootloader: number;
  tee: number;
  snp: number;
  microcode: number;
  fmc?: number;
}

export interface SnpTcb {
  current: SnpTcbVersion;
  reported: SnpTcbVersion;
  committed: SnpTcbVersion;
  launch: SnpTcbVersion;
}

export interface AttestationPolicy {
  /** Expected dstack app id (the DstackApp contract address), 40 hex characters. */
  appId: string;
  /** DstackKms contract address that allowlists OS images. */
  kmsContract: string;
  /** JSON-RPC endpoints for the chain hosting both contracts. */
  rpcUrls?: readonly string[];
  /** Accepted platforms. Default `['sev-snp']`. */
  platforms?: readonly string[];
  /** Reject a platform whose reported, current, committed and launch TCB differ. */
  requireUpToDateTcb?: boolean;
  /** Minimum reported firmware versions. */
  minTcb?: Partial<SnpTcbVersion>;
  /**
   * `'auto'`: checked when the runtime and the deployment support it.
   * `'require'`: must verify in Node.js; a browser proceeds and reports `'unsupported'`.
   * `'strict'`: must verify everywhere, so it fails in browsers. `'off'`: skipped.
   */
  tlsBinding?: TlsBindingMode;
}

/** AMD VCEK and ASK certificates, each as DER bytes, hex or PEM. */
export interface SnpCollateral {
  vcek: Uint8Array | string;
  ask: Uint8Array | string;
}

/** Policy overrides and transport options for `verifyAttestation`. */
export interface VerifyAttestationOptions extends Partial<AttestationPolicy> {
  /** Time allowed for each request, and for retrying a busy endpoint. Default 20000. */
  timeoutMs?: number;
  kdsBaseUrl?: string;
  collateral?: SnpCollateral;
  /** A `fetch` to use for every request. Supplying one turns off TLS binding in Node.js. */
  fetch?: typeof fetch;
  /** Milliseconds since the epoch, for certificate validity. */
  now?: number;
  /** Extra options for Node.js TLS connections. */
  tls?: Record<string, unknown>;
}

/** What offline verification of SEV-SNP evidence proves. */
export interface SnpVerification {
  platform: 'sev-snp';
  /** dstack app id bound by the report. */
  appId: string;
  /** Hash of the app-compose document the VM runs. */
  composeHash: string;
  /** dstack OS image identity the launch measurement reproduces. */
  osImageHash: string;
  instanceId: string | null;
  /** SHA-256 of the chip id (the dstack device id). */
  deviceId: string;
  /** Firmware-assigned id of this guest instance; equal across reports from one running VM. */
  reportId: string;
  /** Signed launch measurement, hex. */
  measurement: string;
  /** Signed report data, hex. */
  reportData: string;
  /** AMD processor family. */
  product: string | null;
  tcb: SnpTcb;
  tcbStatus: 'UpToDate' | 'OutOfDate';
}

/** What `verifyAttestation` proves about an origin. */
export interface AttestationResult extends SnpVerification {
  origin: string;
  /** The nonce this verification sent, hex. */
  nonce: string;
  /** Milliseconds since the epoch. */
  verifiedAt: number;
  tlsBinding: 'verified' | 'unavailable' | 'unsupported' | 'skipped';
  /** SHA-256 of the attested TLS public key, hex. Null unless `tlsBinding` is `'verified'`. */
  tlsKeySha256: string | null;
}

/** A decoded attestation envelope. Untrusted until `verifySnpEvidence` has checked it. */
export interface DecodedAttestation {
  platform: string;
  report?: Uint8Array;
  certChain?: unknown[];
  mrConfig?: string;
  config?: string;
}

/** A parsed SNP report. Untrusted until its signature is verified. */
export interface ParsedSnpReport {
  version: number;
  product: string;
  policy: bigint;
  vmpl: number;
  signatureAlgorithm: number;
  platformInfo: bigint;
  keyInfo: number;
  reportData: Uint8Array;
  measurement: Uint8Array;
  hostData: Uint8Array;
  reportId: Uint8Array;
  chipId: Uint8Array;
  tcb: SnpTcb;
  signed: Uint8Array;
  signatureR: Uint8Array;
  signatureS: Uint8Array;
}

/** Public Base mainnet RPC endpoints, tried in order. Pin your own for stronger guarantees. */
export const BASE_PUBLIC_RPC_URLS: readonly string[];
/** Production policy. */
export const LIT_PRODUCTION_POLICY: Readonly<AttestationPolicy>;
/** Staging ("next") policy. */
export const LIT_STAGING_POLICY: Readonly<AttestationPolicy>;
/** Production API origin served by the platform-attested deployment. */
export const LIT_API_URL: 'https://api2.chipotle.litprotocol.com';
/** Staging API origin served by the platform-attested deployment. */
export const LIT_STAGING_API_URL: 'https://test2.chipotle.litprotocol.com';
/** Origins this SDK verifies by default, each with its policy. */
export const ATTESTED_ORIGINS: Readonly<Record<string, Readonly<AttestationPolicy>>>;
/** Origins being retired, mapped to their successor. This SDK does not verify them. */
export const LEGACY_ORIGINS: Readonly<Record<string, string>>;
/** AMD Key Distribution Service. It sends no CORS headers, so browsers need server-supplied collateral. */
export const AMD_KDS_BASE_URL: 'https://kdsintf.amd.com';
/** `code` on the error a pinned connection raises when the server's key is not the attested one. */
export const TLS_PIN_MISMATCH: 'ERR_LIT_TLS_PIN_MISMATCH';

/**
 * Verifies that `baseUrl` is the governed Lit workload in a genuine
 * confidential VM. Resolves with what was proven; rejects with an
 * `AttestationError` otherwise. Never resolves on a partial check.
 */
export function verifyAttestation(baseUrl: string, options?: VerifyAttestationOptions): Promise<AttestationResult>;

/**
 * A `fetch` for Node.js whose every connection must present an attested TLS
 * key (`tlsKeySha256` from a verified result). A mismatch rejects with an
 * error whose `code` is `TLS_PIN_MISMATCH`, before any request bytes are sent.
 * Responses are buffered. Redirects are returned, not followed.
 */
export function createPinnedFetch(
  tlsKeySha256: string | readonly string[],
  options?: { tls?: Record<string, unknown> },
): Promise<(input: string | URL, init?: RequestInit) => Promise<Response>>;

/**
 * Checks that the TLS certificate `origin` presents was issued to a key held
 * inside the VM that `expected` describes. Node.js only.
 */
export function verifyTlsBinding(
  origin: string,
  expected: Pick<SnpVerification, 'appId' | 'composeHash' | 'osImageHash' | 'reportId'>,
  options?: {
    timeoutMs?: number;
    collateral?: SnpCollateral;
    kdsBaseUrl?: string;
    fetch?: typeof fetch;
    now?: number;
    tls?: Record<string, unknown>;
    tlsKeySha256?: string | null;
  },
): Promise<{ status: 'verified'; tlsKeySha256: string } | { status: 'unavailable' | 'unsupported' }>;

/**
 * Verifies decoded SEV-SNP evidence offline: hardware chain, report signature,
 * report-data binding, launch policy, launch measurement and app identity.
 * It does not consult the chain: see `verifyAttestation` for governance.
 */
export function verifySnpEvidence(
  evidence: DecodedAttestation,
  options: { expectedReportData: Uint8Array; collateral: SnpCollateral; now?: number },
): Promise<SnpVerification>;

/** Resolves the policy for an origin: explicit options over the built-in table. */
export function resolvePolicy(
  baseUrl: string,
  overrides?: Partial<AttestationPolicy>,
): { origin: string; policy: AttestationPolicy };

/** Decodes a dstack VersionedAttestation. Nothing in the result is trusted until verified. */
export function decodeAttestation(bytes: Uint8Array): DecodedAttestation;
/** Parses an SNP attestation report. Fields are untrusted until the signature is verified. */
export function parseSnpReport(report: Uint8Array): ParsedSnpReport;
/** URL of the VCEK certificate AMD issues for a report's chip and reported firmware. */
export function amdKdsVcekUrl(parsedReport: ParsedSnpReport, kdsBaseUrl?: string): string;
/**
 * Recomputes the SNP launch measurement from the launch inputs the VM declares.
 * The inputs are untrusted: compare the result with the signed measurement.
 */
export function computeSnpLaunch(vmConfigJson: string): Promise<{
  measurement: Uint8Array;
  osImageHash: string;
  cmdline: string;
  vcpus: number;
  mrConfig: unknown;
}>;
/** The 64 report-data bytes a v3 attestation for `nonce` (32 bytes) must carry. */
export function attestationV3ReportData(nonce: Uint8Array): Uint8Array;
/** Keccak-256 (Ethereum's hash, not SHA3-256). */
export function keccak256(data: Uint8Array): Uint8Array;
