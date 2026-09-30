# Dashboard auth browser checks

Requires Node.js 20+ and Python 3. From this directory:

```sh
npm ci
npx playwright install chromium
npm test
```

Tests serve `lit-static` locally and mock API/CDN responses. They cover direct
URLs, reload and browser history, keyboard controls, validation, API sign-in,
account creation and key display, sign-out, and mobile light/dark layouts.
No real accounts, wallet transactions, or payments are created.
