import { primordials } from 'ext:core/mod.js';
import {
  TCPWrap,
  PipeWrap,
  op_listen_unsupported,
} from 'ext:core/ops';

const {
  Error,
  NumberIsInteger,
  ObjectDefineProperty,
  ObjectFreeze,
  ObjectGetOwnPropertyDescriptor,
  RangeError,
  ReflectApply,
  String,
  StringPrototypeStartsWith,
} = primordials;

// Actions can replace global built-ins after snapshot creation. Keep validation
// and native delegation on captured primordials so callbacks cannot obtain the
// hidden native functions or bypass the guards.

// Native hardenings that the ordinary op middleware cannot express.
//
// deno_core 0.401 applies middleware only to plain `#[op2]` declarations;
// CppGC object methods (TCPWrap/PipeWrap) and constructor statics (TTY.isTTY)
// are registered separately, and `process.binding()` hands out Node's raw
// internal modules. Everything below runs while the startup snapshot is built,
// before any action code can capture a native function, and is intentionally
// fail-closed: a missing expected member aborts the snapshot build instead of
// silently leaving a path open. Runtime must NOT re-evaluate this module.

// 1. Node listeners. TCPWrap.bind checks the hostname before resolving it, so
// process.binding('tcp_wrap').TCP could bind "localhost" and listen on
// loopback despite the IP deny list. Sealing the shared prototypes also covers
// aliases, subclasses and borrowed methods; no original is retained here.
for (const [name, ctor, methods] of [
  ['TCPWrap', TCPWrap, ['bind', 'bind6', 'bindWithFlags', 'listen', 'open']],
  ['PipeWrap', PipeWrap, ['bind', 'listen', 'open']],
]) {
  for (const method of methods) {
    const descriptor = ObjectGetOwnPropertyDescriptor(ctor.prototype, method);
    if (typeof descriptor?.value !== 'function') {
      throw new Error(`Missing sandboxed native method: ${name}.${method}`);
    }
    ObjectDefineProperty(ctor.prototype, method, {
      value: op_listen_unsupported,
      writable: false,
      configurable: false,
    });
  }
}
// `open(fd)` adopts a host socket without bind/listen or any permission check.
// Outbound connect/connect6 and resource-table checked open_from_rid stay.

// 2. Node's TTY binding. `TTY.isTTY(-1)` and `new TTY(-1)` hit
// `assert!(fd >= 0)` in deno_node, which unwinds out of a native callback and
// aborts the process (SIGABRT). `new TTY(fd)` with fd > 2 also skipped the
// ReadAll/WriteAll gate Deno applies in `op_tty_check_fd_permission`.
const processObject = globalThis.process;
if (typeof processObject?.binding !== 'function') {
  throw new Error('Missing sandboxed process.binding surface');
}
const nativeBinding = processObject.binding;

const MAX_I32 = 0x7fffffff;

// Define rather than assign: `Error.prototype` is shared with the captured
// `Error`, so an action-installed `name` setter would swallow an assignment.
// The null-prototype descriptor ignores fields added to `Object.prototype`.
function setNotCapableName(error) {
  ObjectDefineProperty(error, 'name', {
    __proto__: null,
    value: 'NotCapable',
    writable: true,
    enumerable: true,
    configurable: true,
  });
}

function isInt32(value) {
  return NumberIsInteger(value) && value >= 0 && value <= MAX_I32;
}

// Hardened eagerly, while the snapshot is built, not on first lookup. Node
// creates TTY handles during bootstrap (`process.stdout._handle` when stdio is
// a terminal, and node:tty streams), and every one of them exposes
// `handle.constructor`. Deferring this to the first `process.binding` call
// would leave those handles pointing at the asserting native constructor.
const nativeTtyModule = ReflectApply(nativeBinding, processObject, ['tty_wrap']);
const NativeTTY = nativeTtyModule?.TTY;
if (typeof NativeTTY !== 'function' || typeof NativeTTY.isTTY !== 'function') {
  throw new Error('Missing sandboxed native TTY binding');
}
const nativeIsTTY = NativeTTY.isTTY;

// A plain function, not a class: validate before the native constructor runs,
// then return the real native handle so every method still works.
function TTY(fd) {
  if (!isInt32(fd)) {
    throw new RangeError(
      `The argument 'fd' must be a non-negative integer. Received ${String(fd)}`,
    );
  }
  if (fd > 2) {
    // Same contract as Deno's own TTY constructor. Node's binding never
    // checked permissions, so an action could wrap any inherited fd.
    const error = new Error(
      'Requires read access to "node:tty TTY()", run again with the --allow-read flag',
    );
    setNotCapableName(error);
    throw error;
  }
  return new NativeTTY(fd);
}

// Point the native prototype at the wrapper so instances (including handles
// Node created before this module ran) cannot hand back the native function.
ObjectDefineProperty(NativeTTY.prototype, 'constructor', {
  value: TTY,
  writable: false,
  configurable: false,
});
// Neutralise the native static too: if a reference to the native constructor
// ever escapes, `isTTY` must still refuse out-of-range fds.
ObjectDefineProperty(NativeTTY, 'isTTY', {
  value: function isTTY(fd) {
    return isInt32(fd) ? ReflectApply(nativeIsTTY, NativeTTY, [fd]) : false;
  },
  writable: false,
  configurable: false,
});
ObjectDefineProperty(TTY, 'isTTY', {
  value: function isTTY(fd) {
    return NativeTTY.isTTY(fd);
  },
  writable: false,
  configurable: false,
});
ObjectFreeze(TTY);

// Replace `TTY` on the cached module object itself rather than on a copy:
// `getBinding` returns this exact object, and every route into it - including
// `require("internal/test/binding").internalBinding("tty_wrap")` - shares the
// reference, so a hardened copy would leave the native constructor reachable.
ObjectDefineProperty(nativeTtyModule, 'TTY', {
  value: TTY,
  writable: false,
  configurable: false,
  enumerable: true,
});
ObjectFreeze(nativeTtyModule);

// 3. `Module._load` returns anything listed in `nativeModuleExports` without
// calling an op, and that table includes `internal/*`. Denying `createRequire`
// therefore did not close the raw-binding route:
// `new (process.getBuiltinModule("module"))("").require(
// "internal/test/binding")` returned the raw `internalBinding` getter, and the
// module's `_load` ESM export is a captured function that a `_load`-only patch
// would miss. `_load` always resolves through `Module._resolveFilename`, so
// guard resolution itself.
const nodeModule = processObject.getBuiltinModule?.('module');
if (nodeModule && typeof nodeModule._resolveFilename === 'function') {
  const isInternalSpecifier = (specifier) =>
    typeof specifier === 'string' &&
    (StringPrototypeStartsWith(specifier, 'internal/') ||
      StringPrototypeStartsWith(specifier, 'node:internal/'));

  const requireDenied = () => {
    const error = new Error('Module resolution is not supported in this context.');
    setNotCapableName(error);
    return error;
  };

  const nativeResolveFilename = nodeModule._resolveFilename;
  const guardedResolveFilename = function _resolveFilename(...args) {
    if (isInternalSpecifier(args[0])) {
      throw requireDenied();
    }
    return ReflectApply(nativeResolveFilename, this, args);
  };

  ObjectDefineProperty(nodeModule, '_resolveFilename', {
    value: guardedResolveFilename,
    writable: false,
    configurable: false,
  });

  // Belt and braces for direct `_load` calls on this object. If the property
  // cannot be redefined the resolver guard above still covers the call path.
  try {
    const nativeLoad = nodeModule._load;
    if (typeof nativeLoad === 'function') {
      ObjectDefineProperty(nodeModule, '_load', {
        value: function _load(...args) {
          if (isInternalSpecifier(args[0])) {
            throw requireDenied();
          }
          return ReflectApply(nativeLoad, this, args);
        },
        writable: false,
        configurable: false,
      });
    }
  } catch {
    // `_load` is resolved dynamically through `_resolveFilename`; leaving it
    // untouched does not reopen the guarded path.
  }
}

// `getBinding` looks the module up with `modules[name]`, which coerces the
// key, so a strict `name === 'tty_wrap'` test is not a boundary:
// `process.binding(new String('tty_wrap'))` would otherwise return the raw
// module and its native TTY. Normalise once, then use the same key for both
// the test and the lookup.
function normalizeBindingName(name) {
  if (typeof name === 'string' || typeof name === 'symbol') {
    return name;
  }
  return String(name);
}

ObjectDefineProperty(processObject, 'binding', {
  value: function binding(name) {
    const key = normalizeBindingName(name);
    if (key === 'tty_wrap') {
      return nativeTtyModule;
    }
    return ReflectApply(nativeBinding, this, [key]);
  },
  writable: false,
  configurable: false,
});
