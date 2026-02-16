#!/bin/bash

set -e -o pipefail

rm -rf build docs
mkdir build docs

LIT_SDK=ext/js/02_litActionsSDK.js
LIT_AUTH_SDK=ext/js/04_litAuthDocs.js
LIT_GLOBALS=ext/js/05_globalsDocs.js

echo "Building API docs... Run `npm install` to install the dependencies if you get an error about not finding the documentation tool."
# Build API docs
documentation build "$LIT_SDK" "$LIT_AUTH_SDK" "$LIT_GLOBALS" -f md --config documentation.yml -o docs/api_docs.md --project-name "Lit Actions SDK"
documentation build "$LIT_SDK" "$LIT_AUTH_SDK" "$LIT_GLOBALS" -f html --config documentation.yml -o docs/api_docs_html --project-name "Lit Actions SDK"

# Post-process documentation output to ensure Lit.Actions prefixes appear in navigation and headings.
node <<'NODE'
const fs = require('fs');
const path = require('path');

const root = process.cwd();
const sdkPath = path.join(root, 'ext/js/02_litActionsSDK.js');
const htmlPath = path.join(root, 'docs/api_docs_html/index.html');
const markdownPath = path.join(root, 'docs/api_docs.md');

function loadLitActionsNames() {
  const source = fs.readFileSync(sdkPath, 'utf8');
  const match = source.match(/globalThis\.LitActions\s*=\s*\{([\s\S]*?)\};/);
  if (!match) {
    return [];
  }
  const names = [];
  match[1]
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith('//'))
    .forEach((line) => {
      const identifierMatch = line.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*(?=,|$)/);
      if (identifierMatch) {
        names.push(identifierMatch[1]);
      }
    });
  return names;
}

const actionNames = loadLitActionsNames();

function prefixHtml(content, name) {
  const prefixed = `Lit.Actions.${name}`;
  const headingRegex = new RegExp(`(<h3[^>]*>\\s*)${name}(\\s*</h3>)`, 'g');
  const linkRegex = new RegExp(`(<a[^>]*>\\s*)${name}(\\s*</a>)`, 'g');
  const signatureRegex = new RegExp(`(class='pre[^>]*>\\s*)${name}(?=\\()`, 'g');
  return content
    .replace(headingRegex, `$1${prefixed}$2`)
    .replace(linkRegex, `$1${prefixed}$2`)
    .replace(signatureRegex, `$1${prefixed}`);
}

function reorderAuthNavigation(html) {
  const tocRootIndex = html.indexOf("<div id='toc'>");
  if (tocRootIndex === -1) {
    return html;
  }
  const listStart = html.indexOf('<ul', tocRootIndex);
  const listEnd = html.indexOf('</ul>', listStart);
  if (listStart === -1 || listEnd === -1) {
    return html;
  }
  const listEndWithTag = listEnd + '</ul>'.length;
  const before = html.slice(0, listStart);
  let toc = html.slice(listStart, listEndWithTag);
  const after = html.slice(listEndWithTag);

  const litAuthItems = [];
  const authItemRegex = /<li><a[^>]*>\s*Lit\.Auth(?:\.[^<]*)?\s*<\/a>\s*<\/li>\s*/g;
  toc = toc.replace(authItemRegex, (match) => {
    litAuthItems.push(match.trim());
    return '';
  });

  if (!litAuthItems.length) {
    return before + toc + after;
  }

  const authHeadingRegex = /(\s*<li><a[^>]*href='#auth-utilities'[^>]*>[^<]*<\/a>\s*<\/li>)/;
  toc = toc.replace(authHeadingRegex, (match) => {
    const indentMatch = match.match(/^\s*/);
    const indent = indentMatch ? indentMatch[0] : '';
    const formatted = litAuthItems
      .map((item) => `\n${indent}${item}`)
      .join('');
    return match + formatted;
  });

  return before + toc + after;
}

function prefixMarkdown(content, name) {
  const prefixed = `Lit.Actions.${name}`;
  const headingRegex = new RegExp(`(#+\\s+)${name}(\\b)`, 'g');
  const codeCallRegex = new RegExp('`' + name + '\\(', 'g');
  const listRegex = new RegExp(`(^\\s*[-*]\\s+)${name}(\\b)`, 'gm');
  return content
    .replace(headingRegex, `$1${prefixed}$2`)
    .replace(codeCallRegex, '`' + prefixed + '(')
    .replace(listRegex, `$1${prefixed}$2`);
}

if (fs.existsSync(htmlPath)) {
  let html = fs.readFileSync(htmlPath, 'utf8');
  for (const name of actionNames) {
    html = prefixHtml(html, name);
  }
  html = reorderAuthNavigation(html);
  fs.writeFileSync(htmlPath, html);
}

if (fs.existsSync(markdownPath)) {
  let markdown = fs.readFileSync(markdownPath, 'utf8');
  for (const name of actionNames) {
    markdown = prefixMarkdown(markdown, name);
  }
  fs.writeFileSync(markdownPath, markdown);
}
NODE

# Generate types.d.ts from JSDoc (https://www.typescriptlang.org/docs/handbook/declaration-files/dts-from-js.html)
sed '/^import /d' "$LIT_SDK" > build/sdk.js
tsc build/sdk.js --declaration --allowJs --emitDeclarationOnly

cat > build/types.d.ts <<EOF
export declare namespace Lit {
  export namespace Actions {
$(sed 's/declare //' build/sdk.d.ts)
  }

  export namespace Auth {

    /**
     * Stack of action IPFS IDs tracking the call hierarchy.
     * When a parent action calls a child action, the child's IPFS ID is pushed onto this stack.
     * @type {Array<string>}
     */
    const actionIpfsIdStack: Array<string>;

    /**
     * The address from the authentication signature.
     * @type {string | null}
     */
    const authSigAddress: string | null;

    /**
     * Array of authentication method contexts.
     * @type {Array<{
     *   userId: string;
     *   appId: string;
     *   authMethodType: number;
     *   lastRetrievedAt: string;
     *   expiration: number;
     *   usedForSignSessionKeyRequest: boolean;
     * }>}
     */
    const authMethodContexts: {
      userId: string;
      appId: string;
      authMethodType: number;
      lastRetrievedAt: string;
      expiration: number;
      usedForSignSessionKeyRequest: boolean;
    }[];

    /**
     * Array of resources from the SIWE message or session signature.
     * @type {Array<string>}
     */
    const resources: Array<string>;

    /**
     * Custom authentication resource string.
     * The template literal type represents a string of the form: "\"\\(true,${string})\\\""
     * Example: "\"\\(true,exampleValue)\\\""
     * @type {string | \`"\\\\(true,\${string})\\\\"\`}
     */
    const customAuthResource: string | \`"\\\\(true,\${string})\\\\"\`;
  }
}

/**
 * Global reference to Lit.Actions namespace for convenience.
 * This is identical to using Lit.Actions.
 */
declare const LitActions: typeof Lit.Actions;

/**
 * Global reference to Lit.Auth namespace for convenience.
 * This is identical to using Lit.Auth.
 */
declare const LitAuth: typeof Lit.Auth;

/**
 * The ethers.js v5 library for interacting with Ethereum and other EVM chains.
 * Includes utilities for wallets, contracts, providers, and cryptographic operations.
 * See https://docs.ethers.io/v5/ for full documentation.
 * 
 * For full type definitions, install: npm install --save-dev ethers@5
 * Then import types with: import type { ethers } from 'ethers';
 */
declare const ethers: typeof import('ethers');

/**
 * The jsonwebtoken library for JWT encoding, decoding, and verification.
 * See https://github.com/auth0/node-jsonwebtoken for full documentation.
 */
declare const jwt: {
  decode: (token: string, options?: any) => any;
  verify: (token: string, secretOrPublicKey: string | Buffer, options?: any) => any;
  sign: (payload: string | object | Buffer, secretOrPrivateKey: string | Buffer, options?: any) => string;
};
EOF

prettier build/types.d.ts --write --no-config --ignore-path

cp build/types.d.ts docs/types.d.ts

# copy it over so it's available on the public web
cp build/types.d.ts docs/api_docs_html/
mkdir -p packages/naga-la-types
cp build/types.d.ts packages/naga-la-types/types.d.ts

echo "Hey you! You need to manually copy docs/types.d.ts to lit-js-sdk and also to js-sdk."
