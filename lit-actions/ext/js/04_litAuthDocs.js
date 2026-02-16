/**
 * Authentication Context available inside a Lit Action via `Lit.Auth`.
 *
 * This namespace is injected at runtime and is read-only. It contains
 * contextual information about the current execution and authentication.
 *
 * @namespace Lit.Auth
 */

/**
 * Stack of action IPFS IDs tracking the parent/child call hierarchy.
 * When a parent action calls a child action, the child's IPFS ID is pushed onto this stack.
 * @name Lit.Auth.actionIpfsIdStack
 * @memberof Lit.Auth
 * @type {Array<string>}
 */
const actionIpfsIdStack = [];

/**
 * The address derived from the authentication signature, if present; otherwise null.
 * @name Lit.Auth.authSigAddress
 * @memberof Lit.Auth
 * @type {string|null}
 */
const authSigAddress = null;

/**
 * Array of authentication method contexts used for this execution.
 * @name Lit.Auth.authMethodContexts
 * @memberof Lit.Auth
 * @type {Array<{userId: string, appId: string, authMethodType: number, lastRetrievedAt: string, expiration: number, usedForSignSessionKeyRequest: boolean}>}
 */
const authMethodContexts = [];

/**
 * Array of resources (URIs) from SIWE/session signatures associated with this execution.
 * @name Lit.Auth.resources
 * @memberof Lit.Auth
 * @type {Array<string>}
 */
const resources = [];

/**
 * Custom authentication resource string.
 * @name Lit.Auth.customAuthResource
 * @memberof Lit.Auth
 * @type {string}
 */
const customAuthResource = "";
