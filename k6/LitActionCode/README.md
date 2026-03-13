# Lit Action Code

Shared Lit Action JavaScript for k6 tests. Import from `LitActionCode/index.ts`:

```ts
import { HELLO_WORLD_CODE, ECDSA_SIGN_CODE, ENCRYPT_CODE, DECRYPT_CODE } from "../LitActionCode/index.ts";
```

| Export | Description |
|--------|-------------|
| `HELLO_WORLD_CODE` | Simple `Lit.Actions.setResponse` |
| `ECDSA_SIGN_CODE` | Signs "Chipotle Rocks!" with `Lit.Actions.getLitActionPrivateKey`; returns `wallet_address`, `signature`, `publicKey` |
| `ENCRYPT_CODE` | `Lit.Actions.Encrypt` — expects `pkpId`, `challenge` via `js_params` |
| `DECRYPT_CODE` | `Lit.Actions.Decrypt` — expects `pkpId`, `ciphertext` via `js_params` |

The `.js` files are the canonical source; `index.ts` re-exports them for k6 use (k6 can't import .js).
