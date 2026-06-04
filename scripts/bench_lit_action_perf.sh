#!/usr/bin/env bash
set -euo pipefail

# Benchmark the imported micro-eth-signer Lit Action that has been reported as
# slow across two git refs. The script creates temporary worktrees, applies a
# small benchmark patch to lit-actions/tests/bench.rs, and runs Criterion.
#
# Usage:
#   scripts/bench_lit_action_perf.sh [before_ref] [after_ref]
#
# Defaults are origin/main and pr-388. For a GitHub PR ref that has not been
# fetched locally, run:
#   git fetch origin main pull/388/head:pr-388
#   scripts/bench_lit_action_perf.sh origin/main pr-388

BEFORE_REF="${1:-origin/main}"
AFTER_REF="${2:-pr-388}"
TOOLCHAIN="${RUST_TOOLCHAIN_OVERRIDE:-1.92}"
SAMPLE_SIZE="${SAMPLE_SIZE:-10}"
WARM_UP_TIME="${WARM_UP_TIME:-1}"
MEASUREMENT_TIME="${MEASUREMENT_TIME:-3}"
FILTER="${FILTER:-Lit Actions/Imported micro-eth-signer action}"

REPO_ROOT="$(git rev-parse --show-toplevel)"
RUN_ROOT="${RUN_ROOT:-$REPO_ROOT/target/lit-action-perf}"
PATCH_FILE="$RUN_ROOT/lit-action-perf-bench.patch"
mkdir -p "$RUN_ROOT"

cleanup() {
  git -C "$REPO_ROOT" worktree remove --force "$RUN_ROOT/before" >/dev/null 2>&1 || true
  git -C "$REPO_ROOT" worktree remove --force "$RUN_ROOT/after" >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup

cat > "$PATCH_FILE" <<'PATCH'
diff --git a/lit-actions/tests/Cargo.toml b/lit-actions/tests/Cargo.toml
index f8114e4b..7a871f72 100644
--- a/lit-actions/tests/Cargo.toml
+++ b/lit-actions/tests/Cargo.toml
@@ -29,6 +29,7 @@ lit-core = { workspace = true }
 pretty_assertions = "1"
 pretty_env_logger = "0.5"
 rstest = "0.19"
+serde_json = { workspace = true }
 sys_traits = { workspace = true }
 temp-file = "0.1"
 tokio = { workspace = true }
diff --git a/lit-actions/tests/bench.rs b/lit-actions/tests/bench.rs
index 61c5b804..f64b0e29 100644
--- a/lit-actions/tests/bench.rs
+++ b/lit-actions/tests/bench.rs
@@ -1,7 +1,10 @@
 use std::{
     path::{Path, PathBuf},
     rc::Rc,
-    sync::Arc,
+    sync::{
+        Arc,
+        atomic::{AtomicU64, Ordering},
+    },
 };
 
 use anyhow::{Result, bail};
@@ -18,6 +21,7 @@ use deno_runtime::{
 };
 use indoc::indoc;
 use lit_actions_server::{TestServer, init_v8, proto::*, unix};
+
 use sys_traits::impls::RealSys;
 use tokio_stream::StreamExt as _;
 use tonic::Request;
@@ -79,13 +83,74 @@ impl TestClient {
     fn handle_op(&mut self, op: UnionResponse) -> ExecuteJsRequest {
         match op {
             UnionResponse::SetResponse(_) => SetResponseResponse {}.into(),
-            _ => unimplemented!("op not implemented"),
+            UnionResponse::GetPrivateKey(_) => GetPrivateKeyResponse {
+                secret: TEST_PRIVATE_KEY.to_string(),
+            }
+            .into(),
+            UnionResponse::UpdateResourceUsage(_) => UpdateResourceUsageResponse {
+                cancel_action: false,
+            }
+            .into(),
+            _ => unimplemented!("op not implemented: {op:?}"),
         }
     }
 }
 
 static SNAPSHOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/BASE_SNAPSHOT.bin"));
 
+const TEST_PRIVATE_KEY: &str = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
+
+const IMPORTED_SIGNER_ACTION: &str = indoc! {r#"
+    import { eip191Signer } from "micro-eth-signer@0.18.1";
+
+    async function main({pkpId,action,vmType,message}) {
+        const startTime = performance.now();
+        const pkpPrivateKey = await Lit.Actions.getPrivateKey({ pkpId });
+        const endTime = performance.now();
+        const signature = eip191Signer.sign(message, pkpPrivateKey);
+
+        return {
+            vmType: "ethereum-vm",
+            address: `Signature took ${endTime - startTime} milliseconds`,
+            signature,
+            getPrivateKeyMs: endTime - startTime,
+        };
+    }
+"#};
+
+fn imported_signer_request() -> ExecutionRequest {
+    imported_signer_request_with_nonce(None)
+}
+
+static UNIQUE_ACTION_COUNTER: AtomicU64 = AtomicU64::new(0);
+
+fn imported_signer_request_unique() -> ExecutionRequest {
+    let nonce = UNIQUE_ACTION_COUNTER.fetch_add(1, Ordering::Relaxed);
+    imported_signer_request_with_nonce(Some(nonce))
+}
+
+fn imported_signer_request_with_nonce(nonce: Option<u64>) -> ExecutionRequest {
+    let code = match nonce {
+        Some(nonce) => format!("{IMPORTED_SIGNER_ACTION}\n// benchmark nonce {nonce}"),
+        None => IMPORTED_SIGNER_ACTION.to_string(),
+    };
+
+    ExecutionRequest {
+        code,
+        js_params: Some(
+            serde_json::json!({
+                "pkpId": "benchmark-pkp",
+                "action": "sign",
+                "vmType": "ethereum-vm",
+                "message": "0x68656c6c6f20776f726c64"
+            })
+            .to_string()
+            .into_bytes(),
+        ),
+        ..Default::default()
+    }
+}
+
 const ASYNC_CODE: &str = indoc! {r#"
     (async () => {
         const numbers = Array.from({ length: 100 }, (_, i) => i);
@@ -129,6 +194,24 @@ fn lit_actions(c: &mut Criterion) {
         })
     });
 
+    group.bench_function("Imported micro-eth-signer action", |b| {
+        b.to_async(&runtime).iter(|| async {
+            TestClient::new(server.socket_path())
+                .execute_js(black_box(imported_signer_request()))
+                .await
+                .unwrap();
+        })
+    });
+
+    group.bench_function("Imported micro-eth-signer action uncached code", |b| {
+        b.to_async(&runtime).iter(|| async {
+            TestClient::new(server.socket_path())
+                .execute_js(black_box(imported_signer_request_unique()))
+                .await
+                .unwrap();
+        })
+    });
+
     group.bench_function("No code", |b| {
         b.to_async(&runtime).iter(|| async {
             TestClient::new(server.socket_path())
@@ -141,6 +224,7 @@ fn lit_actions(c: &mut Criterion) {
     group.finish();
 }
 
+#[cfg(any())]
 fn vanilla_deno(c: &mut Criterion) {
     init_v8();
 
@@ -195,5 +279,5 @@ fn vanilla_deno(c: &mut Criterion) {
     group.finish();
 }
 
-criterion_group!(benches, lit_actions, vanilla_deno);
+criterion_group!(benches, lit_actions);
 criterion_main!(benches);
PATCH

run_one() {
  local label="$1" ref="$2" wt="$3"
  echo "==> Preparing $label ($ref)"
  git -C "$REPO_ROOT" worktree add --detach "$wt" "$ref" >/dev/null
  git -C "$wt" apply "$PATCH_FILE"
  cargo +"$TOOLCHAIN" fmt --manifest-path "$wt/lit-actions/tests/Cargo.toml"
  echo "==> Benchmarking $label ($ref)"
  (
    cd "$wt"
    CARGO_INCREMENTAL=0 cargo +"$TOOLCHAIN" bench \
      --manifest-path lit-actions/Cargo.toml \
      --bench execute_js \
      -- "$FILTER" \
      --sample-size "$SAMPLE_SIZE" \
      --warm-up-time "$WARM_UP_TIME" \
      --measurement-time "$MEASUREMENT_TIME"
  ) | tee "$RUN_ROOT/$label.log"
}

run_one before "$BEFORE_REF" "$RUN_ROOT/before"
run_one after "$AFTER_REF" "$RUN_ROOT/after"

echo ""
echo "Logs written to:"
echo "  $RUN_ROOT/before.log"
echo "  $RUN_ROOT/after.log"
