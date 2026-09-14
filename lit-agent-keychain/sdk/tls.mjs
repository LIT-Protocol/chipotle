// Node-only helper: observe the TLS certificate an origin presents so the SDK
// can bind it to the attested enclave (see protocol/attestation.ts step 5).
import { connect } from "node:tls";
import { createHash } from "node:crypto";

/** SHA-256 (hex) of the DER leaf certificate served by `origin`, or undefined for http. */
export function peerCertificateSha256(origin, timeoutMs = 10000) {
  const url = new URL(origin);
  if (url.protocol !== "https:") return Promise.resolve(undefined);
  return new Promise((resolve, reject) => {
    const socket = connect({
      host: url.hostname,
      port: Number(url.port || 443),
      servername: url.hostname,
      ALPNProtocols: ["http/1.1"],
    });
    const timer = setTimeout(() => {
      socket.destroy();
      reject(new Error("Attestation: TLS handshake timed out"));
    }, timeoutMs);
    socket.once("secureConnect", () => {
      clearTimeout(timer);
      const cert = socket.getPeerCertificate();
      const authorized = socket.authorized;
      socket.end();
      if (!authorized || !cert?.raw)
        return reject(new Error("Attestation: TLS certificate not trusted"));
      resolve(createHash("sha256").update(cert.raw).digest("hex"));
    });
    socket.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
  });
}
