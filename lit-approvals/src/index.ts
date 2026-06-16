export const VERSION = '0.1.0';

export {
  requestApproval,
  recordSubmission,
  checkApproval,
  verifyApproval,
  ApprovalVerifyError,
  publicKeyHex,
} from './approvals';
export type {
  RequestApprovalInput,
  RequestApprovalResult,
  CheckApprovalInput,
  CheckApprovalResult,
  VerifyApprovalInput,
} from './approvals';

export { neonStore, neonHttpQuery, SCHEMA_SQL } from './store';
export type { NeonQuery } from './store';

export {
  sha256Hex,
  deriveOtpKey,
  otpHmacHex,
  signPayload,
  verifyPayloadSig,
  genApprovalId,
  genOtp,
  defaultRandomBytes,
} from './crypto';
export type { RandomBytes } from './crypto';

export type {
  Assurance,
  RowStatus,
  ApprovalRow,
  ApprovalStore,
  AttestationPayload,
  AttestationEnvelope,
  FetchLike,
} from './types';
