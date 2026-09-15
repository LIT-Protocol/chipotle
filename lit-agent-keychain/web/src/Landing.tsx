import React from "react";

export const REPO_URL =
  "https://github.com/LIT-Protocol/chipotle/tree/main/lit-agent-keychain";
export const NPM_URL = "https://www.npmjs.com/package/@lit-protocol/keychain";
export const DOCS_URL = "https://developer.litprotocol.com";
const MCP_COMMAND =
  "claude mcp add lit-keychain -- npx -y @lit-protocol/keychain mcp ./agent-identity.json ./API_KEY.keychain.json";

export function LandingNav() {
  return (
    <nav className="site-nav" aria-label="Site">
      <a href="#how">How it works</a>
      <a href="#agents">For agents</a>
      <a href="#faq">FAQ</a>
      <a href="/SKILL.md">Docs</a>
    </nav>
  );
}

export function LandingFooter({ contactEmail }: { contactEmail: string }) {
  return (
    <footer className="site-footer">
      <div>
        © {new Date().getFullYear()} Lit Protocol · Open source client · Lit
        Action authorization
      </div>
      <div className="footer-links">
        <a href="https://litprotocol.com">litprotocol.com</a>
        <a href={DOCS_URL}>Developer docs</a>
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

function Faq({ q, children }: { q: string; children: React.ReactNode }) {
  return (
    <details className="faq-item">
      <summary>{q}</summary>
      <div>{children}</div>
    </details>
  );
}

export function Landing() {
  return (
    <div className="landing">
      <section id="how" className="landing-section">
        <p className="eyebrow">HOW IT WORKS</p>
        <h2>Encrypt once. Approve per agent. Lit checks every request.</h2>
        <div className="step-grid">
          <div className="step-card">
            <span className="step-num">01 · ENCRYPT</span>
            <h3>Your browser encrypts the secret</h3>
            <p>
              Every secret gets its own immutable Lit Action with its own key,
              derived inside a hardware enclave. Your browser fetches that
              action’s public key straight from Lit and encrypts locally. Only
              ciphertext reaches our database.
            </p>
          </div>
          <div className="step-card">
            <span className="step-num">02 · APPROVE</span>
            <h3>You authorize an agent’s public key</h3>
            <p>
              The agent generates its own identity and gives you only the public
              half. You approve it for a secret with an expiry of up to 90 days.
              Your wallet, passkey or Google-verified session signs the
              approval; Lit issues a receipt Keychain cannot forge.
            </p>
          </div>
          <div className="step-card">
            <span className="step-num">03 · FETCH</span>
            <h3>The agent asks Lit, not us</h3>
            <p>
              The agent signs a request and sends it directly to the Lit action.
              The action verifies your receipt, the agent’s key, scope and
              expiry, then returns the result encrypted to that agent only.
              Revoke any time from the dashboard.
            </p>
          </div>
        </div>
      </section>

      <section id="agents" className="landing-section">
        <p className="eyebrow">FOR AGENTS</p>
        <h2>Point your coding agent here and it can onboard itself.</h2>
        <p className="section-lede">
          One npm package covers the SDK, a CLI and a local MCP server for
          Claude Code, Codex, Cursor and any other MCP client. The agent’s
          private key never leaves its machine.
        </p>
        <pre className="terminal">
          <code>
            {`npx @lit-protocol/keychain init ./agent-identity.json
# share only the public key, approve it in Keychain, download the agent config
${MCP_COMMAND}`}
          </code>
        </pre>
        <div className="doc-grid">
          <a className="doc-card" href="/SKILL.md">
            <h3>/SKILL.md</h3>
            <p>
              Agent playbook: generate an identity, get approved, read secrets
              at runtime, and tell the three credential files apart.
            </p>
          </a>
          <a className="doc-card" href="/llms.txt">
            <h3>/llms.txt</h3>
            <p>
              Machine-readable index of this service in the llms.txt format.
            </p>
          </a>
          <a className="doc-card" href={NPM_URL}>
            <h3>@lit-protocol/keychain</h3>
            <p>
              SDK, CLI and MCP server. Node 22+, one dependency, attested Lit
              endpoint, Apache-2.0.
            </p>
          </a>
          <a className="doc-card" href={REPO_URL}>
            <h3>Source on GitHub</h3>
            <p>
              Client, API, the immutable actions pinned by content hash, and the
              security contract.
            </p>
          </a>
        </div>
      </section>

      <section id="faq" className="landing-section">
        <p className="eyebrow">FAQ</p>
        <h2>How it works and how we keep you secure.</h2>

        <h3 className="faq-group">The basics</h3>
        <Faq q="What is Lit Agent Keychain?">
          <p>
            A vault for the API keys and credentials your AI agents use. You
            encrypt each secret in your browser, then approve specific agent
            public keys to use it. Agents fetch secrets through Lit Protocol’s
            network of hardware enclaves, which check your signed approval on
            every request. It works through an SDK, a CLI, or any MCP client
            such as Claude Code, Codex or Cursor.
          </p>
        </Faq>
        <Faq q="What is the difference between “Encrypted release” and “Use in Lit only”?">
          <p>
            <strong>Encrypted release</strong> is the default. The action
            decrypts the value inside the enclave and re-encrypts it to the
            requesting agent’s key, so the agent gets the plaintext on its own
            machine and uses it like any other credential.
          </p>
          <p>
            <strong>Use in Lit only</strong> never reveals the value to anyone.
            The action uses the credential inside the enclave to make one fixed
            API call and returns only a bounded result. Today that covers
            reading a Stripe balance; the action cannot call any other URL,
            export the key, or run caller-supplied code. More strict
            integrations will follow.
          </p>
        </Faq>
        <Faq q="What does the agent actually hold?">
          <p>
            Two files. An <strong>agent identity</strong>, an Ed25519 key pair
            the agent generates locally; only the public key is ever shared. And
            an <strong>agent config</strong> you download after approving it,
            which lists the approved secrets and includes a scoped execution key
            that pays for Lit execution. That execution key is a billing
            credential. It cannot read a secret without the agent’s private key
            and your signed approval.
          </p>
          <p>
            If an agent’s machine is compromised, the attacker can read the
            secrets that agent was approved for until you revoke it, exactly as
            with any credential on a compromised host. Revoke from the
            dashboard, replace the execution key, and prefer “Use in Lit only”
            for keys that should never leave the enclave.
          </p>
        </Faq>

        <h3 className="faq-group">Signing in and custody</h3>
        <Faq q="How does “Sign in with Google” actually work here?">
          <p>
            Your browser first generates a fresh Ed25519 session key that lives
            only in memory and expires after ten minutes. A hash of that public
            key and a random value is passed to Google as the OpenID nonce, so
            the ID token Google returns is bound to this specific session key.
          </p>
          <p>
            When you approve something, whether a new secret, an agent, or a
            policy change, your browser signs that exact object with the session
            key and sends the signature together with the Google ID token to an
            immutable Lit Action running inside a hardware enclave. The action
            fetches Google’s public keys, verifies the token’s signature,
            issuer, audience, subject, expiry and nonce, and only then signs a
            durable receipt for that object. Our database stores the receipt. It
            never stores your Google token, and the token cannot mint new
            receipts after it expires.
          </p>
          <p>
            Your identity is Google’s stable subject ID for your account, not
            your email address.
          </p>
        </Faq>
        <Faq q="Google versus wallet or passkey: what is the tradeoff?">
          <p>
            <strong>Google</strong> is the easiest path: nothing to back up, no
            device dependency, and account recovery through Google. The cost is
            that your Google account becomes the root of trust for your vault.
            Anyone who takes over that account, and Google itself, can obtain a
            token for your subject and authorize new agents. That is the same
            trust every “Sign in with Google” button implies, but here it gates
            your secrets.
          </p>
          <p>
            <strong>Wallet and passkey</strong> are self-custody. Every approval
            needs a signature from hardware or a device only you hold. Keychain,
            Google and Lit cannot produce one. The cost is that losing the key
            means losing access unless you have approved a second owner
            credential.
          </p>
          <p>
            Our recommendation: start with whichever is convenient, then open
            Recovery &amp; backups and add a passkey or wallet as a second
            approved owner credential before storing anything valuable.
          </p>
        </Faq>
        <Faq q="Why does self-custody matter at all? Why not just a password?">
          <p>
            Because everything hinges on who can authorize an agent. In a
            conventional secrets manager the server makes that decision, so a
            database breach, a stolen admin session or a rogue insider can grant
            access to anything. Keychain is built so that nothing on our side
            can do that.
          </p>
          <p>
            The Lit Action releases a secret only when it sees a receipt that
            traces back to your credential, and our servers never hold a key
            that can produce one. If our entire database is dumped, the attacker
            gets ciphertext and signed policies they cannot extend. That
            guarantee exists only because the authorizing key is yours: a
            wallet, a passkey, or your Google account verified inside the
            enclave with the tradeoffs described above.
          </p>
        </Faq>
        <Faq q="What if I lose my passkey or wallet?">
          <p>
            Under Recovery &amp; backups you can approve additional owner
            credentials and download an encrypted backup and recovery
            descriptor. On a new device, choose “Recover an existing vault”,
            load the descriptor and sign in with any approved credential.
          </p>
          <p>
            If your only credential is gone and you have no descriptor, nobody
            can recover the vault, including us. That is the point of
            self-custody, so add a second credential early.
          </p>
        </Faq>

        <h3 className="faq-group">The security model</h3>
        <Faq q="What is the “derived action key” model and why is it secure?">
          <p>
            Each secret gets its own immutable Lit Action: a fixed code template
            plus a small manifest naming your vault, the secret and its release
            mode. Your browser computes the content hash of that code, its IPFS
            CID, locally. Inside the enclave, Lit derives a signing key that is
            unique to that exact CID; change one byte of code and you get a
            different key. The action derives its X25519 encryption key from
            that root.
          </p>
          <p>
            Your browser asks Lit directly for the action’s public key, checks
            Lit’s signature on it, and encrypts the secret to that key. The only
            thing that can decrypt it is that exact program, running in an
            attested enclave. And that program’s code says: verify the owner’s
            receipt, verify the agent’s signature, check scope and expiry, then
            encrypt the result to the agent’s key. There is no admin path around
            those checks because no one else holds the key. Neither Keychain nor
            Lit’s operators can decrypt outside the action. The pattern is
            documented in the{" "}
            <a href={`${DOCS_URL}/lit-actions/derived-actions`}>
              Derived Actions
            </a>{" "}
            docs.
          </p>
        </Faq>
        <Faq q="What can Keychain, the operator, see or do?">
          <p>
            <strong>We can see</strong> secret names, agent public keys,
            policies and request metadata. We never see plaintext values, your
            owner keys, or agent private keys.
          </p>
          <p>
            <strong>We can</strong> deny service, delete ciphertext, or serve an
            older still-valid policy. That last one matters: within a policy’s
            lifetime, a malicious operator could undo a revocation by replaying
            the previous signed policy. Policies default to 30 days and are
            capped at 90.
          </p>
          <p>
            <strong>We cannot</strong> forge an approval, add an agent, extend
            an expiry, or read a secret. The full contract is in{" "}
            <a href="/SECURITY.md">SECURITY.md</a>.
          </p>
        </Faq>
        <Faq q="How does the agent know it is talking to real Lit hardware?">
          <p>
            Before its first request, the SDK, CLI and MCP server verify a
            remote attestation from the Lit endpoint: an Intel TDX quote chained
            to Intel’s pinned root certificate, a replay of the boot event log,
            a check that the measured application matches the release
            whitelisted on-chain by Lit’s governance, and in Node, a binding of
            the live TLS certificate to the enclave. Any failure blocks the
            request.{" "}
            <a href={`${DOCS_URL}/architecture/verification/attestation`}>
              How Lit attestation works
            </a>
            .
          </p>
        </Faq>
        <Faq q="How fast is revocation, and what are its limits?">
          <p>
            Revoking writes a new signed policy, and the action fetches the
            current policy on every request, so the next request from that agent
            is denied. Three limits apply: a request already in flight may
            finish, plaintext an agent already received cannot be recalled, and
            an operator could replay the old policy until it expires. Keep
            policies short and rotate the underlying credential if you suspect
            it leaked.
          </p>
        </Faq>
        <Faq q="Is it open source? Can I verify what runs?">
          <p>
            Yes. The client, API, SDK and every Lit Action are Apache-2.0 in the{" "}
            <a href={REPO_URL}>chipotle repository</a>. Actions are pinned by
            content hash, so you can read the exact code allowed to touch your
            vault. One honest caveat: the hosted web client is part of the trust
            boundary, since open source does not prove a server served an
            audited build. For a stronger separation from us, run a verified
            client release yourself.
          </p>
        </Faq>

        <h3 className="faq-group">Pricing</h3>
        <Faq q="What does it cost?">
          <p>
            Free for five secrets, with every sign-in method, agent access,
            rotation and recovery included. $10 per month for up to 1,000
            secrets. Execution is included under fair use with no automatic
            overage charges, and rotating a secret does not use another slot.
            Cancelling never deletes your secrets or encrypted backups. For more
            secrets or high-volume usage, contact us.
          </p>
        </Faq>
      </section>
    </div>
  );
}
