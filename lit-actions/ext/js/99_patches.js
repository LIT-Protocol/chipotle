import {
  op_eval_context,
  op_increment_fetch_count,
  op_panic,
} from 'ext:core/ops';

// Import modules to suppress build error:
// "Following modules were not evaluated; make sure they are imported from other code"
// This is required because we currently extend globalThis instead of using ES modules at runtime.
import * as _ethers from 'ext:lit_actions/00_ethers.js';
import * as _viem from 'ext:lit_actions/00_viem.js';
import * as _actions from 'ext:lit_actions/02_litActionsSDK.js';

// this block scopes oldFetch so that nobody can ever use it after
{
  const oldFetch = globalThis.fetch;
  const fetch = async function () {
    const fetchCount = await op_increment_fetch_count();
    // console.log(
    //   "fetchCount: " +
    //     fetchCount +
    //     " and arguments: " +
    //     JSON.stringify(arguments, null, 2)
    // );
    return oldFetch.apply(null, arguments);
  };
  Object.freeze(fetch);

  globalThis.fetch = fetch;
}

// Expose Deno's built-in panic op for testing
globalThis.LitTest = { op_panic };

// Route user code through op_eval_context so V8's eval-context code cache
// (wired into WorkerServiceOptions.v8_code_cache) sees it. execute_script
// bypasses that cache, so on repeated executions V8 reparses and recompiles
// the bundled action from source every time (CPL-264). The outer stub still
// parses per execution but its body is one string literal, so V8 only pays
// for source-string scanning, not for compiling the bundled action body.
//
// The caller passes a content-derived `specifier` (the action's IPFS id) so
// distinct actions occupy distinct code-cache keys; see the call site in
// runtime.rs for why a shared specifier would let equal-length actions collide.
//
// The helper deletes itself from globalThis as its first action so user code
// (which runs inside op_eval_context below) cannot reach it; this preserves
// the `--disallow-code-generation-from-strings` posture by not handing actions
// a string-eval primitive. Each request runs in a fresh worker, so the delete
// has no cross-request effect.
globalThis.__litEvalCached = (source, specifier) => {
  delete globalThis.__litEvalCached;
  // `op_eval_context` returns `[result, [thrown, isNativeError, isCompileError]]`.
  // The wrapper in `ext:core/01_core.js` reshapes it into an object, but we're
  // calling the raw op here so we must index by position.
  const [, error] = op_eval_context(source, specifier);
  if (error) {
    throw error[0];
  }
};

// expose "global" because it was available in the old deno version
// but is not available in the new one, and we don't want to break
// existing code that expects it to be available
globalThis.global = globalThis;

// Prime ethers' secp256k1 base-point precompute table at snapshot-build time.
//
// ethers 5.7 (elliptic.js) builds a windowed base-point multiplication table
// lazily, on the FIRST EC operation in an isolate. Merely evaluating the
// library (the import at the top of this file) does not trigger it -- only an
// actual key-derivation/signature op does. Because every Lit Action runs in a
// fresh, one-shot isolate, an un-primed snapshot makes every signing request
// pay that precompute again (~100-150ms on the production TEE CPU; measured
// ~45-50ms cold vs ~2ms warm locally on the runtime's exact bundle). Doing one
// throwaway op here runs it ONCE at snapshot-build time; the resulting table
// hangs off the curve singleton (reachable from the live ethers module) and is
// serialized into the startup snapshot, so every isolate boots warm. Verified:
// in a fresh isolate the ethers-sign extra cost drops from ~40ms to ~13ms.
//
// Kept at the END of this file on purpose: the integration tests pin the line
// number of __litEvalCached in asserted stack traces, so new code goes here to
// avoid shifting it. Position is otherwise irrelevant -- the ethers import is
// hoisted, so the curve is available regardless.
//
// Synchronous on purpose: snapshot-time evaluation does not drive the async
// event loop, so we prime via the synchronous primitives -- the SigningKey
// constructor derives the public key (g.mul, i.e. base-point precompute) and
// signDigest is a synchronous ECDSA sign that warms the signing path -- rather
// than the async Wallet.signMessage(). The key below is a well-known non-secret
// test vector; nothing derived from it is retained.
//
// viem is intentionally not primed here: its bundle uses lazy esbuild module
// initializers and is not exposed as a runtime global, so the same trick does
// not apply cleanly. See CPL follow-up.
try {
  const _warmupKey =
    '0x0000000000000000000000000000000000000000000000000000000000000001';
  const _sk = new _ethers.utils.SigningKey(_warmupKey);
  _sk.signDigest('0x' + '11'.repeat(32));
} catch (_) {
  // Best effort: a runtime/API drift must never break the snapshot build.
  // Worst case we fall back to today's lazy-precompute-per-request cost.
}
