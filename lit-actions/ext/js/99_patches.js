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

// Prime ethers' secp256k1 base-point precompute at snapshot-build time.
//
// ethers 5.7 (elliptic.js) lazily builds a windowed base-point multiplication
// table the first time the secp256k1 curve is constructed (elliptic's EC
// constructor calls g.precompute()). Evaluating the library into the snapshot
// (the import at the top of this file) does NOT construct that curve -- only an
// actual key/signature op does, and ethers caches the curve in a module-level
// singleton on first use. Because every Lit Action runs in a fresh, one-shot
// isolate, an un-primed snapshot makes the first signing op in EACH isolate
// rebuild the table (~100-150ms on the production TEE CPU; measured ~45-50ms
// cold vs ~2ms warm locally on the runtime's exact bundle). Constructing one
// SigningKey here forces that curve + precompute ONCE, at snapshot-build time;
// the cached curve is serialized into the startup snapshot so every isolate
// boots warm. Verified A/B in a fresh isolate: ethers-sign extra cost dropped
// from ~40ms to ~13ms, i.e. the table does survive snapshot serialization.
//
// The load-bearing op is the SigningKey construction (it builds and caches the
// curve). The follow-up signDigest is belt-and-suspenders: it also exercises
// the deterministic-nonce (RFC 6979) sign path, and by returning a real
// signature it lets us record below whether the warmup actually executed.
//
// __litSecp256k1Warmed is a build-time success marker: true only if the sign
// path ran end-to-end and produced a signature. A regression test asserts it
// is true in a fresh isolate (see secp256k1_precompute_warmed_in_snapshot in
// tests/it.rs), so a silent failure -- ethers API drift renaming
// SigningKey/signDigest, or the op throwing -- becomes a loud CI failure
// instead of quietly reshipping the per-request cold-start cost.
//
// Kept at the END of this file on purpose: the integration tests assert
// __litEvalCached's stack frame; appending here avoids churn. Position is
// otherwise irrelevant -- the ethers import is hoisted.
//
// The key below is a well-known non-secret test vector; nothing derived from
// it is retained. ECDSA here is deterministic (RFC 6979), so no entropy enters
// the snapshot. viem is intentionally NOT primed: its bundle uses lazy esbuild
// module initializers and is not exposed as a runtime global, so the same trick
// does not apply cleanly (CPL follow-up).
let _secp256k1Warmed = false;
try {
  const _warmupKey =
    '0x0000000000000000000000000000000000000000000000000000000000000001';
  const _sk = new _ethers.utils.SigningKey(_warmupKey);
  const _sig = _sk.signDigest('0x' + '11'.repeat(32));
  _secp256k1Warmed = !!(_sig && _sig.r);
} catch (_) {
  // Graceful: an ethers API drift must never break the snapshot build. The
  // marker stays false so the regression test fails loudly instead.
}
globalThis.__litSecp256k1Warmed = _secp256k1Warmed;
