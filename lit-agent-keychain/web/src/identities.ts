import { parseGoogleApproval } from "../../protocol/google-approval.ts";
import {
  googleSessionSchema,
  ownerSchema,
  type GoogleApproval,
  type Owner,
  type GoogleSession,
  type Authority,
} from "../../protocol/schema.ts";
import {
  OwnerClient,
  LitConnection,
  authorizationTypedData,
  type OwnerSigner,
  type Challenge,
} from "../../sdk/src/index.ts";
import {
  b64u,
  unb64u,
  randomBytes,
  randomId,
  unhex,
  hex,
  digest,
  agentPublicKey,
  signAgent,
  nowSeconds,
} from "../../protocol/crypto.ts";
import { jsonFetch } from "../../protocol/client-http.ts";
export type Identity = {
  owner: Owner;
  signer: OwnerSigner;
  remember?: (issued?: {
    approval: GoogleApproval;
    authorityRelease: string;
  }) => void;
};
/** Thrown by a signer whose Google approval session has lapsed; the vault stays
 *  open and the app asks for a fresh Google approval before retrying. */
export class GoogleSessionExpired extends Error {
  constructor() {
    super("Google approval session expired. Approve with Google again.");
    this.name = "GoogleSessionExpired";
  }
}
/** Public descriptors only: which credential signed in and the vault it opened.
 *  The server session cookie is the actual proof; this lets a refresh rebuild the
 *  client without a new sign-in. Google device approvals are stored separately. */
const SESSION_KEY = "keychain.session";
export type StoredSession = { v: 2; owner: Owner; authority: Authority };
export function saveSession(owner: Owner, authority: Authority) {
  const session: StoredSession = { v: 2, owner, authority };
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
}
export function loadSession(): StoredSession | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (parsed?.v !== 2 || !parsed.owner || !parsed.authority) return null;
    return parsed as StoredSession;
  } catch {
    return null;
  }
}
export function clearSession() {
  localStorage.removeItem(SESSION_KEY);
  localStorage.removeItem(GOOGLE_KEY);
}
/** Whether the server still honours the session cookie for the stored vault. */
export async function sessionAlive(session: StoredSession): Promise<boolean> {
  try {
    const me = await jsonFetch("/api/me", { credentials: "include" });
    return me?.vaultId === digest(session.authority);
  } catch {
    return false;
  }
}
export const LIT_URL =
  import.meta.env.VITE_LIT_API_URL || "https://api.chipotle.litprotocol.com";
export function authorityFor(owner: Owner, network: string): Authority {
  return { v: 2, network, registry: window.location.origin, owner };
}
export function walletIdentity(
  address: string,
  signTypedData: (data: any) => Promise<`0x${string}`>,
): Identity {
  const owner: Owner = { kind: "wallet", address: address.toLowerCase() };
  return {
    owner,
    signer: async (challenge) => ({
      kind: "wallet",
      owner,
      challenge,
      signature: await signTypedData(authorizationTypedData(challenge)),
    }),
  };
}
// NotAllowedError deliberately does not distinguish cancellation, timeout and
// an unavailable credential. Never infer that the user's passkey was deleted.
async function requestPasskey(
  request: Promise<Credential | null>,
): Promise<PublicKeyCredential> {
  try {
    const credential = await request;
    if (!credential) throw new DOMException("No credential", "NotAllowedError");
    return credential as PublicKeyCredential;
  } catch (error) {
    if (error instanceof DOMException && error.name === "NotAllowedError") {
      throw new Error(
        "Passkey request cancelled, timed out, or unavailable. Try again and choose a passkey available on this device or another device. If you lost a passkey, use an approved recovery passkey with 'Use an existing passkey', or load your vault backup under 'Recover an existing vault' and sign in with an approved credential.",
        { cause: error },
      );
    }
    throw error;
  }
}
export async function createPasskey(label: string): Promise<Identity> {
  const credential = await requestPasskey(
    navigator.credentials.create({
      publicKey: {
        rp: { name: "Lit Agent Keychain", id: location.hostname },
        user: { id: randomBytes(), name: label, displayName: label },
        challenge: randomBytes(),
        pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        attestation: "none",
        authenticatorSelection: {
          residentKey: "required",
          userVerification: "required",
        },
        timeout: 60000,
      },
    }),
  );
  if (!credential) throw new Error("Passkey creation cancelled");
  const response = credential.response as AuthenticatorAttestationResponse;
  const spki = response.getPublicKey();
  if (!spki || response.getPublicKeyAlgorithm() !== -7)
    throw new Error("This authenticator must support P-256");
  const key = await crypto.subtle.importKey(
    "spki",
    spki,
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["verify"],
  );
  const owner: Owner = {
    kind: "passkey",
    publicKey: hex(new Uint8Array(await crypto.subtle.exportKey("raw", key))),
    credentialId: b64u(new Uint8Array(credential.rawId)),
    rpId: location.hostname,
    origin: location.origin,
  };
  localStorage.setItem("keychain.passkey", JSON.stringify(owner));
  return passkeyIdentity(owner);
}
export function passkeyIdentity(
  owner: Extract<Owner, { kind: "passkey" }>,
): Identity {
  return {
    owner,
    signer: async (challenge: Challenge) => {
      const credential = await requestPasskey(
        navigator.credentials.get({
          publicKey: {
            challenge: unhex(digest(challenge)),
            rpId: owner.rpId,
            allowCredentials: [
              { type: "public-key", id: unb64u(owner.credentialId, 1024) },
            ],
            userVerification: "required",
            timeout: 60000,
          },
        }),
      );
      if (!credential) throw new Error("Passkey approval cancelled");
      if (b64u(new Uint8Array(credential.rawId)) !== owner.credentialId)
        throw new Error("Wrong passkey");
      const response = credential.response as AuthenticatorAssertionResponse;
      return {
        kind: "passkey",
        owner,
        challenge,
        clientDataJSON: b64u(new Uint8Array(response.clientDataJSON)),
        authenticatorData: b64u(new Uint8Array(response.authenticatorData)),
        signature: b64u(new Uint8Array(response.signature)),
      };
    },
  };
}
export async function discoverPasskey(recovery?: Authority): Promise<{
  identity: Identity;
  authority?: Authority;
}> {
  // A cached descriptor is not a vault selection: adding a recovery passkey
  // overwrites it. Always discover the selected credential and resolve its vault.
  // The random discovery assertion is NOT login; signer still approves the
  // actual challenge and the authority action verifies membership and possession.
  const credential = await requestPasskey(
    navigator.credentials.get({
      publicKey: {
        challenge: randomBytes(),
        rpId: location.hostname,
        userVerification: "required",
        timeout: 60000,
      },
    }),
  );
  if (!credential) throw new Error("Passkey sign-in cancelled");
  const id = b64u(new Uint8Array(credential.rawId));
  // A backup carries the root's public descriptor even if the backend account
  // is gone. Match the selected credential exactly; this does not authorize it.
  // Recovery members are discovered after restoreCredentials restores the policy.
  if (recovery?.owner.kind === "passkey" && recovery.owner.credentialId === id)
    return { identity: passkeyIdentity(recovery.owner), authority: recovery };
  const result = await jsonFetch(`/api/passkeys/${id}`);
  const owner = result.owner as Extract<Owner, { kind: "passkey" }>;
  localStorage.setItem("keychain.passkey", JSON.stringify(owner));
  return { identity: passkeyIdentity(owner), authority: result.authority };
}
const GOOGLE_KEY = "keychain.google-approval";
type GoogleDevice = {
  owner: Extract<Owner, { kind: "google" }>;
  session: GoogleSession;
  token: string;
  privateKey: string;
  issued?: { approval: GoogleApproval; authorityRelease: string };
};
function googleIdentity(device: GoogleDevice): Identity {
  const privateKey = unhex(device.privateKey);
  return {
    owner: device.owner,
    remember(issued) {
      if (issued) {
        if (
          digest(issued.approval.payload.owner) !== digest(device.owner) ||
          issued.approval.payload.session.publicKey !== device.session.publicKey
        )
          throw new Error("Google approval identity mismatch");
        device.issued = issued;
      }
      localStorage.setItem(GOOGLE_KEY, JSON.stringify(device));
    },
    signer: async (challenge, authorityRelease) => {
      const issued =
        device.issued?.authorityRelease === authorityRelease
          ? device.issued
          : undefined;
      const session = issued?.approval.payload.session ?? device.session;
      if (challenge.expiresAt > session.expiresAt)
        throw new GoogleSessionExpired();
      return {
        kind: "google",
        owner: device.owner,
        challenge,
        session,
        token: issued ? "" : device.token,
        ...(issued ? { approval: issued.approval } : {}),
        signature: signAgent(challenge, privateKey),
      };
    },
  };
}
export function restoreGoogleIdentity(
  owner: Owner,
  network: string,
): Identity | undefined {
  try {
    const raw = localStorage.getItem(GOOGLE_KEY);
    if (!raw) return;
    const device: GoogleDevice = JSON.parse(raw);
    const storedOwner = ownerSchema.parse(device.owner);
    const session = googleSessionSchema.parse(device.session);
    if (
      storedOwner.kind !== "google" ||
      digest(storedOwner) !== digest(owner) ||
      session.network !== network ||
      session.registry !== location.origin ||
      !/^[0-9a-f]{64}$/.test(device.privateKey) ||
      agentPublicKey(unhex(device.privateKey)) !== session.publicKey ||
      typeof device.token !== "string" ||
      device.token.length > 8192
    )
      return;
    if (device.issued) {
      const approval = parseGoogleApproval(device.issued.approval);
      const approved = approval.payload.session;
      if (
        digest(approval.payload.owner) !== digest(owner) ||
        approved.network !== network ||
        approved.registry !== location.origin ||
        approved.publicKey !== session.publicKey ||
        typeof device.issued.authorityRelease !== "string"
      )
        return;
      if (approved.expiresAt <= nowSeconds()) {
        localStorage.removeItem(GOOGLE_KEY);
        return;
      }
    } else if (session.expiresAt <= nowSeconds()) return;
    // Local storage isn't trusted as authorization: Lit verifies the certificate,
    // exact lifetime, owner membership and a new challenge signature every time.
    return googleIdentity(device);
  } catch {
    return;
  }
}
export function googleSession(network: string) {
  const privateKey = randomBytes();
  const now = nowSeconds();
  const session: GoogleSession = {
    v: 2,
    domain: "lit-keychain/google-session/v2",
    network,
    registry: location.origin,
    publicKey: agentPublicKey(privateKey),
    nonce: randomId(),
    issuedAt: now,
    // Historical immutable authorities cap Google proofs at 15 minutes.
    // A fresh login exchanges this proof for a separate 30-day device approval.
    expiresAt: now + 600,
    scope: "authorize",
  };
  return {
    session,
    nonce: b64u(unhex(digest(session))),
    identity(token: string, clientId: string): Identity {
      const claims = JSON.parse(
        new TextDecoder().decode(unb64u(token.split(".")[1], 8192)),
      );
      const owner = ownerSchema.parse({
        kind: "google",
        subject: claims.sub,
        clientId,
      });
      if (owner.kind !== "google") throw new Error("Invalid Google owner");
      return googleIdentity({
        owner,
        session,
        token,
        privateKey: hex(privateKey),
      });
    },
    destroy() {
      privateKey.fill(0);
    },
  };
}
export function ownerClient(
  identity: Identity,
  network: string,
  recovery?: Authority,
) {
  return new OwnerClient(
    recovery || authorityFor(identity.owner, network),
    identity.signer,
    new LitConnection(LIT_URL),
  );
}
