import { version } from "../../sdk/package.json";
// Single source for the SDK version shown in copyable commands. Markdown docs
// and llms.txt are held to the same value by tests/docs.test.ts.
export const SDK_PACKAGE = "@lit-protocol/keychain";
export const SDK_VERSION: string = version;
export const NPX_KEYCHAIN = `npx ${SDK_PACKAGE}@${SDK_VERSION}`;
