// Site chrome and the home page intro. The explanatory material (how it works,
// FAQ, security model, pricing detail) lives on developer.litprotocol.com so
// this page can stay small: a short introduction beside the sign-in surface.
import React from "react";

export const REPO_URL =
  "https://github.com/LIT-Protocol/chipotle/tree/main/lit-agent-keychain";
export const NPM_URL = "https://www.npmjs.com/package/@lit-protocol/keychain";
export const DOCS_URL = "https://developer.litprotocol.com";
export const KEYCHAIN_DOCS_URL = `${DOCS_URL}/keychain`;

/** The Lit wordmark, drawn in the current text color. */
export function LitMark({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 311 228"
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M311 104.987V51.9125H256.038V29.2084L256.245 0.621826H202.816V174.264C202.816 181.242 204.193 188.153 206.866 194.599C209.54 201.045 213.459 206.9 218.398 211.83C223.337 216.76 229.2 220.667 235.652 223.328C242.103 225.989 249.016 227.352 255.994 227.338L311 227.25V175.045H269.794C267.969 175.047 266.162 174.689 264.477 173.992C262.791 173.295 261.259 172.272 259.969 170.982C258.679 169.692 257.656 168.16 256.959 166.474C256.262 164.789 255.904 162.982 255.906 161.157V140.517H256.053C256.053 128.723 256.053 116.929 256.053 104.943L311 104.987Z" />
      <path d="M142.841 51.9125H184.564V0.621826H131.489V227.442H184.564V93.9711C184.564 88.7506 182.208 83.8089 178.151 80.5223L142.841 51.9125Z" />
      <path d="M53.2347 161.157V0.621826H0.160156V174.264C0.160143 181.242 1.53637 188.153 4.21006 194.599C6.88376 201.045 10.8024 206.9 15.7418 211.83C20.6811 216.76 26.5442 220.667 32.9954 223.328C39.4466 225.989 46.3593 227.352 53.3379 227.338L113.12 227.25V175.045H67.1225C63.4392 175.045 59.9068 173.582 57.3023 170.978C54.6978 168.373 53.2347 164.841 53.2347 161.157Z" />
    </svg>
  );
}

export function Brand() {
  return (
    <a href="/" className="brand" aria-label="Lit Agent Keychain">
      <LitMark />
      <span className="divider" aria-hidden="true" />
      <span>Agent Keychain</span>
    </a>
  );
}

/** Left column of the home page: what this is, in a few lines. */
export function Intro() {
  return (
    <section className="intro">
      <p className="eyebrow">Lit Agent Keychain</p>
      <h1>
        Your agents.
        <br />
        Your keys.
        <br />
        Your permission.
      </h1>
      <p className="lede">
        Encrypt API keys in your browser. Approve exactly which agents may use
        them. Lit checks your authorization on every request, so nobody else can
        grant access, not even us.
      </p>
      <div className="principles">
        <p>
          <span>01</span> Secrets are encrypted on your device. Only ciphertext
          reaches our database.
        </p>
        <p>
          <span>02</span> Access is signed by your wallet, passkey or Google
          account and verified in confidential hardware.
        </p>
        <p>
          <span>03</span> Agents use an SDK, CLI or local MCP server. Revoke any
          agent at any time.
        </p>
      </div>
      <p className="intro-links">
        <a className="text-link" href={KEYCHAIN_DOCS_URL}>
          How Keychain works <span aria-hidden="true">↗</span>
        </a>
        <a className="text-link" href={`${KEYCHAIN_DOCS_URL}/quickstart`}>
          Quickstart <span aria-hidden="true">↗</span>
        </a>
        <a className="text-link" href={`${KEYCHAIN_DOCS_URL}/security`}>
          Security model <span aria-hidden="true">↗</span>
        </a>
      </p>
      <p className="intro-price">
        Free for 5 secrets. $10 per month for up to 1,000. Open source,
        Apache-2.0.
      </p>
    </section>
  );
}

export function LandingFooter({ contactEmail }: { contactEmail: string }) {
  return (
    <footer className="site-footer">
      <div>© {new Date().getFullYear()} Lit Protocol</div>
      <div className="footer-links">
        <a href="https://litprotocol.com">litprotocol.com</a>
        <a href={KEYCHAIN_DOCS_URL}>Docs</a>
        <a href={REPO_URL}>Source</a>
        <a href={NPM_URL}>npm</a>
        <a href="/SKILL.md">SKILL.md</a>
        <a href="/llms.txt">llms.txt</a>
        <a href="/SECURITY.md">Security</a>
        <a href={`mailto:${encodeURIComponent(contactEmail)}`}>Support</a>
      </div>
    </footer>
  );
}
