// Compiled, never run: a strict TypeScript consumer of the packed tarball.
// `release.mjs` type-checks it when LIT_SDK_TYPECHECK is set.
import Client, {
  createClient,
  LIT_API_URL,
  AttestationError,
  type AttestationResult,
  type ClientOptions,
  type LitActionResponse,
} from '@lit-protocol/core-sdk';
import { verifyAttestation, createPinnedFetch, TLS_PIN_MISMATCH } from '@lit-protocol/core-sdk/attestation';

declare const apiKey: string;

const options: ClientOptions = {
  baseUrl: LIT_API_URL,
  attestation: { rpcUrls: ['https://example.invalid'], ttlMs: 60_000, onVerified: (r: AttestationResult) => void r.composeHash },
};
const client: Client = createClient(options);
export const fromUrl: Client = createClient(LIT_API_URL);

export async function run(): Promise<string> {
  const untyped: LitActionResponse = await client.litAction({ apiKey, code: 'x', jsParams: { a: 1 } });
  const typed = await client.litAction<{ address: string }>({ apiKey, code: 'x' });
  const exists: boolean = await client.accountExists(apiKey);
  const keys = await client.listApiKeys({ apiKey, pageSize: 50 });
  const group = await client.addGroup({ apiKey, groupName: 'g' });
  const proof = await client.verifyAttestation();
  const standalone = await verifyAttestation(LIT_API_URL, { tlsBinding: 'require' });
  if (standalone.tlsKeySha256) {
    const pinned = await createPinnedFetch(standalone.tlsKeySha256);
    const res: Response = await pinned(`${LIT_API_URL}/core/v1/version`);
    void res.status;
  }
  try {
    await client.getNodeChainConfig();
  } catch (e) {
    if (e instanceof AttestationError && e.code === 'endpoint_busy') return TLS_PIN_MISMATCH;
  }
  // @ts-expect-error apiKey is required
  await client.litAction({ code: 'x' });
  // @ts-expect-error the response is unknown until a type is given
  untyped.response.address;
  return [typed.response.address, String(exists), keys[0]?.api_key_hash, group.group_id, proof.platform, client.attestation?.origin].join();
}
