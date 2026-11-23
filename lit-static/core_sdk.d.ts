/**
 * Type declarations for `core_sdk.js` (`@lit-protocol/core-sdk`).
 * Written by hand: keep them in step with the JavaScript. `sdk/scripts/types.test.mjs`
 * fails when an export or a public client method is added to one file and not the other.
 */

import type { AttestationResult, VerifyAttestationOptions } from './attestation.js';

export {
  verifyAttestation,
  AttestationError,
  ATTESTED_ORIGINS,
  LEGACY_ORIGINS,
  LIT_API_URL,
  LIT_STAGING_API_URL,
} from './attestation.js';
export type {
  AttestationErrorCode,
  AttestationPolicy,
  AttestationResult,
  VerifyAttestationOptions,
} from './attestation.js';

/** How long a successful attestation is used before a request refreshes it in the background. */
export const DEFAULT_ATTESTATION_TTL_MS: number;
/** Age past which a request waits for a new attestation instead of using the old one. */
export const DEFAULT_ATTESTATION_MAX_AGE_MS: number;

/** Required prefix of every new wallet derivation path, hex. */
export const WALLET_PATH_TAG_HEX: string;
/** True when `derivationPath` (0x-hex or bigint) carries the wallet path tag. */
export function isTaggedWalletPath(derivationPath: string | bigint): boolean;
/**
 * Mint a wallet and register it on-chain, re-minting once if the minted path
 * predates the wallet path tag.
 */
export function mintAndRegisterWallet<M extends { wallet_address: string; derivation_path: string }, R extends object>(
  mint: () => Promise<M>,
  register: (minted: M) => Promise<R>,
): Promise<R & { minted: M }>;

/** Public RPC URL for a known chain ID, or null if the chain is unknown. */
export function rpcUrlForChainId(chainId: number | string | null | undefined): string | null;

// --- ChainSecured (wallet-signed) flows ---

export const PRIMARY_TYPE_CREATE_WALLET: 'CreateWallet';
export const PRIMARY_TYPE_CONVERT_ACCOUNT: 'ConvertAccount';
export const PRIMARY_TYPE_ADD_USAGE_API_KEY: 'AddUsageApiKey';
export const PRIMARY_TYPE_BILLING_AUTH: 'BillingAuth';
export const PRIMARY_TYPE_BILLING_SESSION: 'BillingSession';
export const BILLING_SESSION_MAX_AGE_SECONDS: number;

/** The part of an ethers v6 `Signer` this SDK uses. Any ethers v6 signer fits. */
export interface SignerLike {
  getAddress(): Promise<string>;
  signTypedData(
    domain: Record<string, unknown>,
    types: Record<string, Array<{ name: string; type: string }>>,
    value: Record<string, unknown>,
  ): Promise<string>;
  /** An ethers v6 provider. When present, sovereign reads use it instead of `rpcUrl`. */
  provider?: unknown;
}

/** EIP-712 typed data in the JSON form the server expects. */
export interface ChainSecuredTypedData {
  types: Record<string, Array<{ name: string; type: string }>>;
  primaryType: string;
  domain: { name: string; version: string; chainId: string };
  message: Record<string, string>;
}

export function buildChainSecuredTypedData(options: {
  primaryType: string;
  address: string;
  chainId: number | string;
  issuedAt: number | string;
}): ChainSecuredTypedData;
export function buildBillingSessionTypedData(options: {
  address: string;
  /** EIP-55 checksummed address of the session key. */
  sessionKey: string;
  chainId: number | string;
  issuedAt: number | string;
}): ChainSecuredTypedData;
/** The text a session key signs for one billing request. */
export function billingSessionProofMessage(sessionKey: string, timestamp: number | string): string;
/** Sign canonical typed data with an ethers v6 signer. */
export function signChainSecuredTypedData(
  signer: SignerLike,
  typedData: ChainSecuredTypedData,
): Promise<{ typed_data: ChainSecuredTypedData; signature: string }>;

/** State of a sovereign-mode contract write. */
export type TxState =
  | 'preparing'
  | 'simulating'
  | 'previewing'
  | 'signing'
  | 'pending'
  | 'confirming'
  | 'confirmed'
  | 'failed'
  | 'reorged';

/** Hooks and transaction settings for a sovereign-mode write. Ignored in API mode. */
export interface SovereignLifecycle {
  /** ethers transaction overrides (gasLimit, value, ...). */
  overrides?: Record<string, unknown>;
  /** Confirmations to wait for. Default 1. */
  confirmations?: number;
  /** Called on every state transition. */
  onState?: (state: TxState, payload?: Record<string, unknown>) => void;
  /** Gate before the wallet opens; must resolve true to proceed. */
  onPreview?: (method: string, args: unknown[]) => Promise<boolean>;
}

// --- Client options ---

/** `verifyAttestation` options plus the client's caching settings. */
export interface ClientAttestationOptions extends VerifyAttestationOptions {
  /** Age after which a request starts a background re-verification. Default `DEFAULT_ATTESTATION_TTL_MS`. */
  ttlMs?: number;
  /** Age after which a request waits for re-verification. Default `DEFAULT_ATTESTATION_MAX_AGE_MS`. */
  maxAgeMs?: number;
  /** Called with each verification result. */
  onVerified?: (result: AttestationResult) => void;
}

export interface ClientOptions {
  /** Base URL of the API. Default `LIT_API_URL`. */
  baseUrl?: string;
  /**
   * `'api'` (default): every call goes through the API over HTTP.
   * `'sovereign'`: reads go to the AccountConfig contract and writes are wallet-signed transactions.
   */
  mode?: 'api' | 'sovereign';
  /** RPC URL for sovereign reads when no signer with a provider is attached. */
  rpcUrl?: string;
  /** AccountConfig contract address. Required in sovereign mode. */
  contractAddress?: string;
  /** Expected chain ID. Detected from the RPC when omitted. */
  chainId?: number;
  /** Override map `{ "<chainId>:<address>": { runtimeBytecodeKeccak } }` for drift pinning. */
  deployments?: Record<string, { runtimeBytecodeKeccak: string }>;
  /** Signer for sovereign writes. May be set later with `connectSigner`. */
  signer?: SignerLike;
  /** Account identity hash for ChainSecured sessions: `keccak256(abi.encodePacked(address))`. */
  adminHashOverride?: string;
  /**
   * TEE attestation of the API origin before any request is sent to it.
   * Omitted: verified when `baseUrl` is one of `ATTESTED_ORIGINS`. `false`: never.
   * `true` or an object: always.
   */
  attestation?: boolean | ClientAttestationOptions;
}

// --- Requests ---

/** A group ID, as a decimal or hex string or a number. */
export type GroupId = string | number;

export interface NewAccountOptions {
  accountName: string;
  accountDescription?: string;
  /** Email address for billing. */
  email?: string;
}

export interface LitActionOptions {
  /** Usage API key. */
  apiKey: string;
  /** Lit Action JavaScript code. */
  code: string;
  /** JSON parameters passed to the action. */
  jsParams?: Record<string, unknown> | null;
}

export interface CreateWalletOptions {
  apiKey?: string;
  /** Sovereign mode only. Default `'Wallet'`. */
  name?: string;
  /** Sovereign mode only. Default `'Wallet'`. */
  description?: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface AddGroupOptions {
  apiKey: string;
  groupName: string;
  groupDescription?: string;
  /** PKP IDs permitted to use the group. */
  pkpIdsPermitted?: string[];
  /** Keccak-256 hashes of the permitted action IPFS CIDs. */
  cidHashesPermitted?: string[];
  sovereignLifecycle?: SovereignLifecycle;
}

export interface AddActionOptions {
  apiKey: string;
  actionIpfsCid: string;
  name: string;
  description: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface AddActionToGroupOptions {
  apiKey: string;
  groupId: GroupId;
  actionIpfsCid: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface AddPkpToGroupOptions {
  apiKey: string;
  groupId: GroupId;
  pkpId: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export type RemovePkpFromGroupOptions = AddPkpToGroupOptions;

export interface DeleteWalletOptions {
  /** Must be the account (master) API key. */
  apiKey?: string;
  walletAddress: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface AddUsageApiKeyOptions {
  apiKey: string;
  name: string;
  description: string;
  canCreateGroups?: boolean;
  canDeleteGroups?: boolean;
  canCreatePkps?: boolean;
  /** Group IDs the key may manage action CIDs in. 0 grants nothing. */
  manageIpfsIdsInGroups?: number[];
  /** Group IDs the key may add PKPs to. 0 grants nothing. */
  addPkpToGroups?: number[];
  /** Group IDs the key may remove PKPs from. 0 grants nothing. */
  removePkpFromGroups?: number[];
  /** Group IDs the key may execute actions in. 0 means all groups. */
  executeInGroups?: number[];
  /** Sovereign mode only: unix timestamp in seconds; 0 or omitted means never. */
  expiration?: number | string | bigint;
  /** Sovereign mode only. */
  balance?: number | string | bigint;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface UpdateUsageApiKeyOptions extends AddUsageApiKeyOptions {
  /** The usage API key to update, or its 0x-prefixed Keccak-256 hash (`api_key_hash` from `listApiKeys`). */
  usageApiKey: string;
}

export interface RemoveUsageApiKeyOptions {
  apiKey: string;
  /** The usage API key to remove, or its 0x-prefixed Keccak-256 hash. */
  usageApiKey: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface RemoveGroupOptions {
  apiKey: string;
  groupId: GroupId;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface UpdateGroupOptions {
  apiKey: string;
  groupId: GroupId;
  name: string;
  description: string;
  pkpIdsPermitted?: string[];
  cidHashesPermitted?: string[];
  sovereignLifecycle?: SovereignLifecycle;
}

export interface DeleteActionOptions {
  apiKey: string;
  /** 0x-prefixed Keccak-256 hash of the action's IPFS CID. */
  hashedCid: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface RemoveActionFromGroupOptions extends DeleteActionOptions {
  groupId: GroupId;
}

export interface UpdateActionMetadataOptions extends DeleteActionOptions {
  name: string;
  description: string;
  /** Sovereign mode only. 0 (default) means account-level metadata. */
  groupId?: GroupId;
}

export interface UpdateUsageApiKeyMetadataOptions {
  apiKey: string;
  /** The usage API key to update, or its 0x-prefixed Keccak-256 hash. */
  usageApiKey: string;
  name: string;
  description: string;
  sovereignLifecycle?: SovereignLifecycle;
}

export interface ListPageOptions {
  apiKey: string;
  /** 0-based. Default `'0'`. */
  pageNumber?: string | number;
  /** Default `'10'`. */
  pageSize?: string | number;
}

export interface ListPageWithGroupOptions extends ListPageOptions {
  groupId: GroupId;
}

export interface ListActionsOptions extends ListPageOptions {
  /** When omitted, lists every action on the account. */
  groupId?: GroupId;
}

export interface BillingRequestOptions {
  /** base64(JSON{typed_data, signature}) for ChainSecured auth. When set, `apiKey` is ignored. */
  walletAuthHeader?: string;
  signal?: AbortSignal;
}

// --- Responses ---

export interface NewAccountResponse {
  api_key: string;
  wallet_address: string;
}

export interface CreateWalletResponse {
  wallet_address: string;
  /** Sovereign mode only. */
  transaction_hash?: string;
}

/** Result of running a Lit Action. `T` is the type of what the action returned. */
export interface LitActionResponse<T = unknown> {
  response: T;
  logs: string;
  has_error: boolean;
}

export interface AccountOpResponse {
  success: boolean;
  /** Sovereign mode only. */
  transaction_hash?: string;
}

export interface AddGroupResponse extends AccountOpResponse {
  group_id: string | null;
}

export interface AddUsageApiKeyResponse extends AccountOpResponse {
  /** The new usage API key. Returned only once. */
  usage_api_key: string;
  /** Sovereign mode only: Keccak-256 hash of the key. */
  hash?: string;
}

/** One item from `listGroups` or `listActions`. */
export interface ListMetadataItem {
  id: string;
  name: string;
  description: string;
}

/** One item from `listWallets` or `listWalletsInGroup`. */
export interface WalletItem extends ListMetadataItem {
  wallet_address: string;
}

/** One item from `listApiKeys`. */
export interface ApiKeyItem {
  id: string;
  /** Keccak-256 hash of the usage API key, 0x-prefixed hex. */
  api_key_hash: string;
  name: string;
  description: string;
  /** Unix timestamp, as a string. */
  expiration: string;
  /** API mode: number. Sovereign mode: exact uint256 decimal string. */
  balance: number | string;
  can_create_groups: boolean;
  can_delete_groups: boolean;
  can_create_pkps: boolean;
  can_manage_ipfs_ids_in_groups: number[];
  can_add_pkp_to_groups: number[];
  can_remove_pkp_from_groups: number[];
  can_execute_in_groups: number[];
}

export interface NodeChainConfigResponse {
  chain_name: string;
  chain_id: number;
  is_evm: boolean;
  testnet: boolean;
  token: string;
  /** Filled in from the known-chain map when the API omits it. */
  rpc_url?: string;
  contract_address: string;
}

export interface SupportedLanguage {
  name: string;
  display_name: string;
  execution_model: string;
  runtimes: object[];
  methods: string[];
}

// --- Client ---

export class LitNodeSimpleApiClient {
  constructor(options?: ClientOptions);

  /** The API base URL requests are sent to, ending in `/core/v1`. */
  baseUrl: string;
  /** Origin of `baseUrl`, or null when it could not be resolved. */
  origin: string | null;
  mode: 'api' | 'sovereign';
  rpcUrl: string | null;
  contractAddress: string | null;
  chainId: number | null;
  signer: SignerLike | null;
  adminHashOverride: string | null;

  /** The last successful attestation of the API origin, or null when none has run or attestation is off. */
  readonly attestation: AttestationResult | null;
  /**
   * Verify the API origin now, replacing any cached result. Requests do this
   * themselves when needed; call it to verify eagerly or to read the result.
   */
  verifyAttestation(options?: VerifyAttestationOptions): Promise<AttestationResult>;

  /** Attach a signer for sovereign-mode writes. */
  connectSigner(signer: SignerLike): void;
  /** Non-throwing check that the on-chain contract matches the ABI this SDK was built for. */
  checkAbiDrift(): Promise<{ ok: true } | { ok: false; reason: string }>;

  // Accounts
  newAccount(options: NewAccountOptions): Promise<NewAccountResponse>;
  /** Sovereign mode only. */
  newChainSecuredAccount(options: {
    accountName: string;
    accountDescription?: string;
    sovereignLifecycle?: SovereignLifecycle;
  }): Promise<{ wallet_address: string; api_key_hash: string; transaction_hash: string }>;
  /** Sovereign mode only. */
  transferChainSecuredAccountOwnership(options: {
    newAdminWalletAddress: string;
    sovereignLifecycle?: SovereignLifecycle;
  }): Promise<{ previous_admin: string; new_admin: string; transaction_hash: string }>;
  /** API mode only. Irreversible. */
  convertToChainSecuredAccount(options: {
    apiKey: string;
    signer: SignerLike;
    chainId?: number;
  }): Promise<{ wallet_address: string; api_key_hash: string }>;
  accountExists(apiKey: string): Promise<boolean>;
  /** Sovereign mode only. `apiKeyHashHex` is 0x-prefixed 32-byte hex. */
  accountExistsByHash(apiKeyHashHex: string): Promise<boolean>;

  // Wallets
  /** Accepts an options object or, for backward compatibility, a bare API key. */
  createWallet(options?: string | CreateWalletOptions): Promise<CreateWalletResponse>;
  /** Permanent: the wallet's key can never be derived again. */
  deleteWallet(options: DeleteWalletOptions): Promise<AccountOpResponse>;
  listWallets(options: ListPageOptions): Promise<WalletItem[]>;
  listWalletsInGroup(options: ListPageWithGroupOptions): Promise<WalletItem[]>;
  /** Sovereign mode only. */
  getBillingWalletAddress(options?: { apiKey?: string }): Promise<string>;

  // Actions
  /** The IPFS CID of the given action code. */
  getLitActionIpfsId(code: string): Promise<string>;
  /** Run a Lit Action. Rejects on a non-2xx status. */
  litAction<T = unknown>(options: LitActionOptions): Promise<LitActionResponse<T>>;
  addAction(options: AddActionOptions): Promise<AccountOpResponse>;
  deleteAction(options: DeleteActionOptions): Promise<AccountOpResponse>;
  updateActionMetadata(options: UpdateActionMetadataOptions): Promise<AccountOpResponse>;
  listActions(options: ListActionsOptions): Promise<ListMetadataItem[]>;

  // Groups
  addGroup(options: AddGroupOptions): Promise<AddGroupResponse>;
  updateGroup(options: UpdateGroupOptions): Promise<AccountOpResponse>;
  removeGroup(options: RemoveGroupOptions): Promise<AccountOpResponse>;
  listGroups(options: ListPageOptions): Promise<ListMetadataItem[]>;
  addActionToGroup(options: AddActionToGroupOptions): Promise<AccountOpResponse>;
  removeActionFromGroup(options: RemoveActionFromGroupOptions): Promise<AccountOpResponse>;
  addPkpToGroup(options: AddPkpToGroupOptions): Promise<AccountOpResponse>;
  removePkpFromGroup(options: RemovePkpFromGroupOptions): Promise<AccountOpResponse>;

  // Usage API keys
  addUsageApiKey(options: AddUsageApiKeyOptions): Promise<AddUsageApiKeyResponse>;
  updateUsageApiKey(options: UpdateUsageApiKeyOptions): Promise<AccountOpResponse>;
  updateUsageApiKeyMetadata(options: UpdateUsageApiKeyMetadataOptions): Promise<AccountOpResponse>;
  removeUsageApiKey(options: RemoveUsageApiKeyOptions): Promise<AccountOpResponse>;
  listApiKeys(options: ListPageOptions): Promise<ApiKeyItem[]>;

  // Node
  getNodeChainConfig(): Promise<NodeChainConfigResponse>;
  getSupportedLanguages(): Promise<{ languages: SupportedLanguage[] }>;
  getApiPayers(): Promise<string[]>;
  getAdminApiPayer(): Promise<string>;

  // Billing
  getStripeConfig(): Promise<{ publishable_key: string }>;
  /** `force` bypasses the node's balance cache. */
  getBillingBalance(
    apiKey: string,
    options?: BillingRequestOptions & { force?: boolean },
  ): Promise<{ balance_cents: number; balance_display: string }>;
  /** `amountCents` is in US cents, minimum 500. */
  createPaymentIntent(
    apiKey: string,
    amountCents: number,
    options?: BillingRequestOptions,
  ): Promise<{ client_secret: string; payment_intent_id: string }>;
  confirmPayment(apiKey: string, paymentIntentId: string, options?: BillingRequestOptions): Promise<void>;
}

/** Create a client from a base URL or a full options object. Default `LIT_API_URL`. */
export function createClient(options?: string | ClientOptions): LitNodeSimpleApiClient;

export default LitNodeSimpleApiClient;
