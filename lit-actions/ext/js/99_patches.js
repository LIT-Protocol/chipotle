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
