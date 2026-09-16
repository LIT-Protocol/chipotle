import type { Owner, GoogleSession, Authority } from "../../protocol/schema.ts";
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
export type Identity = { owner: Owner; signer: OwnerSigner };
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
    expiresAt: now + 600,
    scope: "authorize",
  };
  return {
    session,
    nonce: b64u(unhex(digest(session))),
    identity(token: string, clientId: string): Identity {
      // Decoding selects the claimed subject; the action independently verifies
      // Google's signature, audience, expiry, subject, and the session nonce.
      const claims = JSON.parse(
        new TextDecoder().decode(unb64u(token.split(".")[1], 8192)),
      );
      const owner: Owner = { kind: "google", subject: claims.sub, clientId };
      return {
        owner,
        signer: async (challenge) => {
          if (challenge.expiresAt > session.expiresAt)
            throw new Error("Google approval session expired. Sign in again.");
          return {
            kind: "google",
            owner,
            challenge,
            session,
            token,
            signature: signAgent(challenge, privateKey),
          };
        },
      };
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
