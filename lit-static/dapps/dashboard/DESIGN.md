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

Light is a **chalk + charcoal + restrained burnt orange** system (issue #673):
paper surfaces, charcoal ink, orange reserved as an *accent* (links, focus rings,
active states, icon tints). Solid CTAs fill with **ink, not orange** — see
`--btn-primary-bg`. Dark keeps its indigo palette unchanged.

| Token | Light | Dark |
|---|---|---|
| `--primary` (accent) | `#bc3b12` | `#6366f1` |
| `--primary-hover` | `#9e3210` | `#818cf8` |
| `--primary-rgb` (tint channels) | `188, 59, 18` | `99, 102, 241` |
| `--btn-primary-bg` (solid CTA fill) | `#181818` | `#6366f1` |
| `--btn-primary-bg-hover` | `#30302c` | `#818cf8` |
| `--text` | `#181818` | `#f1f5f9` |
| `--text-muted` | `#696963` | `#94a3b8` |
| `--body-bg` | `#fafaf7` | `#0f172a` |
| `--card-bg` | `#ffffff` | `#1e293b` |
| `--border` | `#ddddd5` | `#334155` |
| `--bg-muted` | `#f0f0eb` | `#1e293b` |
| `--danger` | `#dc2626` | `#f87171` |
| `--success` | `#16a34a` | `#4ade80` |
| `--shadow` | whisper (chalk is flat) | darker shadow |

`--primary` is the accent, not the button fill. Primary buttons (`.btn-primary`)
use `--btn-primary-bg` (ink in light, indigo in dark) so orange stays restrained.

Sidebar has its own scoped tokens (`--sidebar-bg`, `--sidebar-text`, etc.) so
the sidebar can stay light when the rest goes dark, or vice versa.

When tinting the primary color (e.g., empty-state icon backgrounds), use
`rgba(var(--primary-rgb), <alpha>)` so the alpha is the only knob and the tint
tracks the theme automatically:

- Icon-background tint: `rgba(var(--primary-rgb), 0.1)`
- Subtle fill: `rgba(var(--primary-rgb), 0.08)`

## Typography

- Body: `"Inter", system-ui, -apple-system, sans-serif`, 14px base, 1.5 line-height.
- Mono: `"JetBrains Mono", ui-monospace, ...` for keys, hashes, addresses.
- Form controls inherit (`font-family: inherit`) — never let browsers pick.

Display headings (the workspace title) use weight **500** with
heavy negative tracking (`-0.04em` to `-0.045em`) for the large, quiet
litprotocol.com feel. Smaller headings use **500-600** with `-0.01em` to
`-0.03em`. Metric numbers are large and light (36px / 400).

Eyebrows (`.eyebrow`) are the recurring section kicker: 10px, uppercase,
`0.1em` tracking, muted. They sit above the workspace title.

Scale (semantic, not pixel-perfect):

| Use | Size | Weight | Tracking |
|---|---|---|---|
| Workspace title (`.workspace-title`) | clamp 30–38px | 500 | -0.04em |
| Sign-in title (`.login-title`) | 28px | 600 | -0.04em |
| Next-step h2 | 21px | 500 | -0.025em |
| Metric value (`.metric-grid .stat-value`) | 36px | 400 | -0.02em |
| Section h2 | 18px | 600 | -0.01em |
| Card title | 0.9375rem (15px) | 600 | — |
| Body | 14px | 400-500 | — |
| Small / labels | 0.8125rem (13px) | 400-500 | — |
| Eyebrow / caption | 10-12px | 500 | 0.1em (eyebrow) |

## Spacing

8px grid. Use multiples: 4, 8, 12, 16, 20, 24, 32. Avoid arbitrary numbers.

Radii:

- `--radius` = 7px (most surfaces)
- `--radius-lg` = 10px (cards, hero blocks, empty-state icon containers, dialogs)

## Layout (issue #673)

The dashboard follows the reviewed litprotocol.com preview: a full-bleed brand
bar, a quiet text sidebar, a flat metric grid, and a page footer. Account access
pages use a standalone centered form.

### App frame

- **Brand bar** (`.app-topbar` on workspace): the
  `Lit` wordmark (`.brand-mark`, orange/indigo accent) + a hairline divider +
  `Dashboard`, on the left. The right side keeps the working controls (mode badge host
  `.topbar-title`, billing balance, Add Funds, Auto recharge, Developer docs,
  theme toggle, Account menu).
  On small screens, the controls wrap below the brand so the Account menu stays
  within the viewport and can be used to sign out.
- **Body** (`.dashboard-body`): sidebar + main content in a row. The sidebar is
  in-flow (not fixed) so the footer can sit below both columns.
- **Footer** (`.app-footer`): "Lit Protocol" left,
  "Confidential, verifiable execution." right. Hairline top border.

### Login layout (`.login-container`)

A centered form with a local Lit logo, a page-specific heading, an account-access
selector, and a link to switch between sign-in and account creation. The form
is at most 440px wide and uses reduced padding on mobile.

### Workspace heading (`.workspace-heading`)

Eyebrow ("Your workspace") + large `Overview` title on the left, one primary
action ("Run an Action") on the right. One primary action per view.

## Components

### Buttons

- `.btn` base. Modifiers: `.btn-primary`, `.btn-outline`, `.btn-sm`, `.btn-block`.
- Primary CTA: solid `--btn-primary-bg` (ink in light, indigo in dark), white
  text. Orange is an accent, never a button fill.
- Outline: transparent background, `--border` border + `--text`.
- Min height 44px on mobile (`max-width: 768px` media query). Desktop can be 36-40px.

### Cards

- Background `--card-bg`, border `1px solid --border`, radius `--radius-lg`,
  shadow `--shadow`. Inner padding 1.5rem (24px).
- Use sparingly. Stacking cards inside cards is a smell.

### Metric grid (`metric-grid` / `metric-cell`)

- Replaces the old boxed stat cards. Flat: four equal cells separated by hairline
  borders (top + bottom on the grid, a left divider between cells), no card box.
- Each cell (`<a>`, not div): small muted label on top, a large light number
  (`.stat-value`, 36px / 400), and a muted sub-label with a `↗` glyph.
- Hover tints the number to `--primary`. Click → smooth-scroll to section.
- `updateStatCards()` (auth.js) fills the numbers and toggles the grid vs the
  all-zero empty state via `[hidden]`. It also mirrors the counts into the
  sidebar (`sidebar-count-*`) and shows/hides the next-step card.

### Next-step card (`next-step`)

- Muted `--bg-muted` panel under the metric grid: a `</>` code glyph +
  "From your rules to a signature." + a link into Groups. Rides with the metric
  grid (hidden in the all-zero empty state).

### Empty states (`empty-state`)

- Centered: 48px brand-tinted icon, title (15px / 600), body (13px / muted, 320px max-width).
- Section-specific copy. Tell the user what the thing is and how to start.
- Toggle visibility via `style.display = 'none' | ''` from JS, not class swap,
  so existing handlers keep working.

### Sidebar (`sidebar`)

- Text-only nav (no icons), in-flow, hairline right border. Tops out with an
  account block (`.sidebar-account`: square mark + "Your account" + mode label),
  and bottoms out with `.sidebar-bottom` ("Open-source ↗").
- Each `.sidebar-link` is a label + an optional right-aligned count
  (`.sidebar-count`, filled by `updateStatCards()`).
- States: default, `:hover` (muted fill), `.is-active` (muted fill, ink text —
  no orange left bar). Active state is set by IntersectionObserver scroll-spy
  (`app.js initSidebar`).
- Under 900px the sidebar becomes a horizontal scroll strip above the content.

### Mode badge (topbar `topbar-mode-badge`)

- Click toggles a `.topbar-mode-popover` that explains the current mode.
- Different copy per mode. ChainSecured popover lists which features are hidden.
- Closes on outside click and Escape.

### Account access pages

- Sign in: `/dapps/dashboard/#sign-in`. Create account:
  `/dapps/dashboard/#create-account`. Native links expose both routes to agents
  and support direct entry, refresh, and browser Back/Forward on static hosting.
- An unauthenticated dashboard visit defaults to sign-in. Successful auth replaces
  the auth route with `#overview`; sign-out replaces it with `#sign-in`.
- Use the official Lit mark from `docs/logo/light.svg`, shipped locally as
  `lit-static/assets/lit-logo.svg`. The logo is not a generic key or shield icon.
- One flat form surface, a page-specific heading, and a persistent primary CTA.
  No nested cards, decorative icons, gradients, or hover-dependent CTA colors.
- Account access selector above the form: Email & password / API key / Wallet.
  The radio group supports arrow keys. Password access is the default on reload
  when configured; API-key and wallet choices remain available.
- Native forms support Enter submission, required fields, and email validation.
  Inactive pages use `hidden`; inactive modes use `display: none`.

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
runs are paid for. ChainSecured users authenticate billing requests via an
EIP-712 typed-data signature (`primaryType: "BillingAuth"`, cached ~4
minutes per session) sent in the `X-Wallet-Auth` header — the dashboard's
`getWalletAuthHeader()` builds the typed data and `BillingAuth` verifies it
server-side. The `primaryType` is part of the EIP-712 type hash, so a
signature minted here cannot be replayed against the secret-emitting
`/add_usage_api_key_with_signature` endpoint or any other ChainSecured flow.

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
3. Add a text `.sidebar-link` with matching `data-scroll="{name}"` (no icon; add a
   `.sidebar-count` span if the section has a countable resource).
4. If it has an empty state, follow the `empty-state` pattern (icon + title + body).
5. If the section is mode-conditional, gate via `body.is-chainsecured` CSS rule.

## Don'ts

- No new CSS frameworks. We have what we need.
- No inline styles for color, spacing, or typography. Use tokens.
- No icon libraries. Inline SVG, lucide-style stroke 2.
- No animations longer than 200ms. Snappy beats smooth.
- No new fonts.
- No "TODO" comments in shipped CSS — file an issue or fix it.

### Password access and auth routing

When the auth service is configured, email/password is the default on every load.
All three methods retain hover/focus explanations and keyboard selection. Native
POST password forms keep their autocomplete, username, and generation hints.
Successful password submission explicitly reloads the dashboard at `#overview`,
so hash routing does not bypass password-manager completion detection.

Verification fragments are consumed before normal login routing changes the URL.
Sign-in/create-account links keep their native hrefs; ordinary clicks update
history synchronously, while Back/Forward update the visible form and title.
Routing and responsive checks run in `e2e/password/routes.spec.ts` alongside the
real Worker/D1 password suite; there is no separate npm browser test package.
