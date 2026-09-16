// Browsers cannot inspect peer certificates. The Node build replaces this
// module with sdk/tls.mjs; no Node imports or runtime detection enter browsers.
export async function peerCertificateSha256(
  _origin: string,
  _timeoutMs?: number,
): Promise<string | undefined> {
  return undefined;
}
