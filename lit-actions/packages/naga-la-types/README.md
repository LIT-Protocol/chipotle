# @lit-protocol/naga-la-types

TypeScript declarations for the Lit Actions runtime. Install this package to get autocompletion and type safety for the global `Lit` namespaces (`Lit.Actions`, `Lit.Auth`) and helper globals such as `LitActions`, `LitAuth`, `ethers`, and `jwt`.

## Install

```bash
npm install @lit-protocol/naga-la-types
# or
yarn add @lit-protocol/naga-la-types
```

## Usage

### Import the namespace types

```ts
import type { Lit } from '@lit-protocol/naga-la-types';

async function sign(params: Parameters<typeof Lit.Actions.signEcdsa>[0]) {
  return await Lit.Actions.signEcdsa(params);
}
```

### Register the globals

If your project relies on the Lit runtime globals, reference the package in `tsconfig.json` so the declarations are picked up automatically:

```json
{
  "compilerOptions": {
    "types": ["@lit-protocol/naga-la-types"]
  }
}
```

Alternatively, import the provided ambient module once during setup:

```ts
import '@lit-protocol/naga-la-types/globals';
```

This augments the global scope with `Lit`, `LitActions`, `LitAuth`, `ethers`, and `jwt` type definitions.
