# Chipotle Dashboard — Design System

What this document is: the source of truth for how the dashboard looks and behaves.
Edit when the system actually changes. Do not edit to describe aspirational state.

## Voice

Functional, calm, builder-to-builder. The dashboard is a control surface for an
API. It should feel like an SDK with a UI on top, not a marketing page.

- Verbs over nouns in CTAs. "Create your first usage key", not "Get started".
- Define jargon on first appearance. "ChainSecured" is not self-explanatory.
- No exclamation points. No emoji. No celebratory copy.
- Errors say what failed and what to do next. They do not apologize.

## Color tokens

All color is driven by CSS custom properties on `:root` (light) and
`[data-theme="dark"]` (dark). Hard-coded hex values are a bug.

| Token | Light | Dark |
|---|---|---|
| `--primary` | `#3c50e0` | `#6366f1` |
| `--primary-hover` | `#5a67d8` | `#818cf8` |
| `--text` | `#1e293b` | `#f1f5f9` |
| `--text-muted` | `#64748b` | `#94a3b8` |
| `--body-bg` | `#f1f5f9` | `#0f172a` |
| `--card-bg` | `#ffffff` | `#1e293b` |
| `--border` | `#e2e8f0` | `#334155` |
| `--bg-muted` | `#f3f4f6` | `#1e293b` |
| `--danger` | `#dc2626` | `#f87171` |
| `--success` | `#16a34a` | `#4ade80` |
| `--shadow` | light shadow | darker shadow |

Sidebar has its own scoped tokens (`--sidebar-bg`, `--sidebar-text`, etc.) so
the sidebar can stay light when the rest goes dark, or vice versa.

When tinting the primary color (e.g., empty-state icon backgrounds), use rgba
with the same RGB as `--primary` so the alpha is the only knob:

- Light tint: `rgba(60, 80, 224, 0.08)`
- Dark tint: `rgba(99, 102, 241, 0.15)`

## Typography

- Body: `"Inter", system-ui, -apple-system, sans-serif`, 14px base, 1.5 line-height.
- Mono: `"JetBrains Mono", ui-monospace, ...` for keys, hashes, addresses.
- Form controls inherit (`font-family: inherit`) — never let browsers pick.

Scale (semantic, not pixel-perfect):

| Use | Size | Weight |
|---|---|---|
| Login title | 28px | 700 |
| Topbar title | 20px | 600 |
| Section h2 | 18px | 600 |
| Card title | 0.9375rem (15px) | 600 |
| Body | 14px | 400-500 |
| Small / labels | 0.8125rem (13px) | 400-500 |
| Caption | 12px | 500 |

## Spacing

8px grid. Use multiples: 4, 8, 12, 16, 20, 24, 32. Avoid arbitrary numbers.

Radii:

- `--radius` = 8px (most surfaces)
- `--radius-lg` = 12px (cards, hero blocks, empty-state icon containers)

## Components

### Buttons

- `.btn` base. Modifiers: `.btn-primary`, `.btn-outline`, `.btn-sm`, `.btn-block`.
- Primary CTA: solid `--primary` background, white text.
- Outline: transparent background, primary border + text.
- Min height 44px on mobile (`max-width: 768px` media query). Desktop can be 36-40px.

### Cards

- Background `--card-bg`, border `1px solid --border`, radius `--radius-lg`,
  shadow `--shadow`. Inner padding 1.5rem (24px).
- Use sparingly. Stacking cards inside cards is a smell.

### Stat cards (`stat-card`)

- Anchor element (`<a>`), not div, so they navigate to their section.
- Layout: icon left (44px square, brand-tinted bg), value + label stacked right.
- Click → smooth-scroll to section + sets sidebar active state.

### Empty states (`empty-state`)

- Centered: 48px brand-tinted icon, title (15px / 600), body (13px / muted, 320px max-width).
- Section-specific copy. Tell the user what the thing is and how to start.
- Toggle visibility via `style.display = 'none' | ''` from JS, not class swap,
  so existing handlers keep working.

### Sidebar link (`sidebar-link`)

- Icon (lucide-style 18px SVG) + text label.
- States: default, `:hover`, `.is-active`.
- Active state: brand color text, soft hover bg, 3px brand bar on the left edge.
- Active state is set by IntersectionObserver scroll-spy (`app.js initSidebar`).

### Mode badge (topbar `topbar-mode-badge`)

- Click toggles a `.topbar-mode-popover` that explains the current mode.
- Different copy per mode. ChainSecured popover lists which features are hidden.
- Closes on outside click and Escape.

### Login mode cards

- Side-by-side on desktop, stacked on mobile.
- API mode card carries the `RECOMMENDED` badge (filled primary) — primary path.
- ChainSecured card carries the `WALLET REQUIRED` badge (muted, neutral fill)
  to set expectation.
- Neither card has a default highlight; both start with the same neutral
  `--border` ring. Hover or focus-within either card → that card gets the
  primary ring while the sibling stays neutral. The badges carry which is
  recommended vs which requires a wallet, so the highlight follows intent
  instead of pre-selecting one.
- Each card's CTA (Log in / Create account / Connect wallet / Connect wallet
  & create) shares the same shape (full-width, btn-block padding) but its
  color follows the card's selected state: neutral outlined at rest, primary
  blue when the card is hovered or focus-within. The btn-primary vs
  btn-outline class no longer drives color here — the parent card does.
- Each card: badge, icon, title, tagline, body, CTA.
- Help glyph next to "ChainSecured" → tooltip defining the term.

### Help disclosure (`details.help-details`)

- Native `<details>` for collapsible explanations (e.g., the dev-doc Instructions block).
- Default closed. The summary acts as the button.

## Mode-conditional UI

The dashboard exposes the same core surface in both modes. Body classes drive
mode-specific gating via CSS:

- `body.has-api-key` — set after successful API-mode login.
- `body.is-chainsecured` — set after successful wallet connect.

Mode-conditional elements use the `.is-chainsecured-only` class — hidden in API
mode via `body:not(.is-chainsecured) .is-chainsecured-only { display: none }`.
Today this gates the **ChainSecured RPC URL** override (Account → RPC URL),
which is meaningless in API mode.

Action Runner, Wallets, Actions, and Usage Keys all render in both modes.
ChainSecured admin operations (add action, mint usage key) are signed by the
connected wallet via the AccountConfig contract. ChainSecured users execute
Lit Actions by pasting a usage API key they minted from the contract.

Billing (balance, Add Funds, no-funds warning, billing banners) is **not**
mode-conditional. Stripe credit funds action runs in both modes. ChainSecured
only changes how admin writes are authorized (wallet vs API key), not how
runs are paid for. ChainSecured users authenticate billing requests via a
SIWE-style EIP-191 signed message (cached ~4 minutes per session) sent in
the `X-Wallet-Auth` header — the dashboard's `getWalletAuthHeader()` builds
the message and `BillingAuth` verifies it server-side. The signature pins
the `Purpose: lit-billing-auth-v1` line to prevent cross-flow replay
against `/create_wallet_with_signature` or `/convert_to_chain_secured_account`.

Validation guards must use `isAuthenticated()`, not `!apiKey` — ChainSecured
users authenticate via wallet and have no account-level api key.

Do not hide via JS-set inline styles. CSS-only toggles are easier to audit
and survive page reloads.

## Accessibility

- All focusable elements get `:focus-visible` outlines (`outline: 2px solid var(--primary)`).
  Keyboard-only — mouse clicks do not trigger the ring.
- Decorative SVGs use `aria-hidden="true"`. Meaningful SVGs get `<title>` or `aria-label`.
- Touch targets ≥ 44px on mobile. Anything below is a bug.
- Popovers use `aria-expanded`, `aria-haspopup`, and `aria-hidden` on the panel.
- Modals use `role="dialog"` and trap focus.

## Adding a new section

1. Add a `<section id="section-{name}" class="dashboard-section">` in `index.html`.
2. Add it to `MAIN_SECTION_IDS` in `app.js` (top of file).
3. Add a sidebar link with matching `data-scroll="{name}"` and a lucide icon.
4. If it has an empty state, follow the `empty-state` pattern (icon + title + body).
5. If the section is mode-conditional, gate via `body.is-chainsecured` CSS rule.

## Don'ts

- No new CSS frameworks. We have what we need.
- No inline styles for color, spacing, or typography. Use tokens.
- No icon libraries. Inline SVG, lucide-style stroke 2.
- No animations longer than 200ms. Snappy beats smooth.
- No new fonts.
- No "TODO" comments in shipped CSS — file an issue or fix it.
