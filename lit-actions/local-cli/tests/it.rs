//! End-to-end tests driving the real `lit` binary, the way an action's
//! shell/python code would: separate process per op, state on disk, keys
//! derived from a pinned local master key.

use std::path::Path;
use std::process::{Command, Output};

const MASTER_KEY: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

/// Run `lit <args>` in `dir` with the given state dir and a pinned key.
fn lit(dir: &Path, state_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lit"))
        .current_dir(dir)
        .env("LIT_LOCAL_PRIVATE_KEY", MASTER_KEY)
        .env("LIT_LOCAL_STATE_DIR", state_dir)
        .args(args)
        .output()
        .expect("failed to run lit")
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

/// A tempdir plus its `.lit-local` state dir.
fn workspace() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join(".lit-local");
    (dir, state)
}

#[test]
fn private_key_is_deterministic_and_per_pkp() {
    let (dir, state) = workspace();
    let d = dir.path();

    let a = stdout(&lit(d, &state, &["get-private-key", "pkp-123"]));
    let b = stdout(&lit(d, &state, &["get-private-key", "pkp-123"]));
    let other = stdout(&lit(d, &state, &["get-private-key", "pkp-999"]));

    assert_eq!(a, b, "same pkp must yield the same key");
    assert_ne!(a, other, "different pkp must differ");
    assert!(a.starts_with("0x") && a.len() == 66, "0x + 64 hex: {a}");
}

#[test]
fn action_key_public_key_and_address_formats() {
    let (dir, state) = workspace();
    let d = dir.path();

    let priv_hex = stdout(&lit(
        d,
        &state,
        &["--ipfs-id", "QmFoo", "get-action-private-key"],
    ));
    let pubkey = stdout(&lit(
        d,
        &state,
        &["--ipfs-id", "QmFoo", "get-action-public-key"],
    ));
    let addr = stdout(&lit(
        d,
        &state,
        &["--ipfs-id", "QmFoo", "get-action-wallet-address"],
    ));

    assert!(priv_hex.starts_with("0x") && priv_hex.len() == 66);
    // Compressed SEC1 point: 33 bytes -> 0x + 66 hex, leading 02/03.
    assert!(
        pubkey.starts_with("0x02") || pubkey.starts_with("0x03"),
        "pubkey: {pubkey}"
    );
    assert_eq!(pubkey.len(), 68);
    // 20-byte address.
    assert!(addr.starts_with("0x") && addr.len() == 42, "addr: {addr}");

    // A different cid yields a different action key.
    let other = stdout(&lit(
        d,
        &state,
        &["--ipfs-id", "QmBar", "get-action-private-key"],
    ));
    assert_ne!(priv_hex, other);
}

#[test]
fn aes_round_trips_and_is_key_scoped() {
    let (dir, state) = workspace();
    let d = dir.path();

    let ct = stdout(&lit(d, &state, &["aes-encrypt", "pkp-1", "hello secret"]));
    assert!(
        ct.chars().all(|c| c.is_ascii_hexdigit()),
        "hex, no 0x: {ct}"
    );
    let pt = stdout(&lit(d, &state, &["aes-decrypt", "pkp-1", &ct]));
    assert_eq!(pt, "hello secret");

    // Decrypting under a different PKP key must fail.
    let wrong = lit(d, &state, &["aes-decrypt", "pkp-2", &ct]);
    assert!(!wrong.status.success());
}

#[test]
fn job_params_and_auth_context_come_from_job_file() {
    let (dir, state) = workspace();
    let d = dir.path();
    std::fs::write(
        d.join("lit.job.json"),
        r#"{"ipfsId":"QmJob","jsParams":{"a":1},"authContext":{"sig":"x"}}"#,
    )
    .unwrap();

    assert_eq!(stdout(&lit(d, &state, &["params"])), r#"{"a":1}"#);
    assert_eq!(stdout(&lit(d, &state, &["auth-context"])), r#"{"sig":"x"}"#);

    let job: serde_json::Value = serde_json::from_str(&stdout(&lit(d, &state, &["job"]))).unwrap();
    assert_eq!(job["ipfsId"], "QmJob");
    assert_eq!(job["jsParams"]["a"], 1);
    assert_eq!(job["timeoutMs"], 900_000);
}

#[test]
fn params_default_to_null_without_a_job_file() {
    let (dir, state) = workspace();
    assert_eq!(stdout(&lit(dir.path(), &state, &["params"])), "null");
}

#[test]
fn fetch_count_persists_across_invocations() {
    let (dir, state) = workspace();
    let d = dir.path();
    assert_eq!(stdout(&lit(d, &state, &["increment-fetch-count"])), "1");
    assert_eq!(stdout(&lit(d, &state, &["increment-fetch-count"])), "2");
    assert_eq!(stdout(&lit(d, &state, &["increment-fetch-count"])), "3");
}

#[test]
fn value_can_be_read_from_stdin() {
    use std::io::Write as _;
    let (dir, state) = workspace();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lit"))
        .current_dir(dir.path())
        .env("LIT_LOCAL_PRIVATE_KEY", MASTER_KEY)
        .env("LIT_LOCAL_STATE_DIR", &state)
        .args(["aes-encrypt", "pkp-1", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"piped-plaintext")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let ct = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    let pt = stdout(&lit(dir.path(), &state, &["aes-decrypt", "pkp-1", &ct]));
    assert_eq!(pt, "piped-plaintext");
}

#[test]
fn missing_key_is_generated_and_reused() {
    let (dir, state) = workspace();
    let d = dir.path();
    // No LIT_LOCAL_PRIVATE_KEY: the CLI generates and persists one.
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lit"))
            .current_dir(d)
            .env("LIT_LOCAL_STATE_DIR", &state)
            .env_remove("LIT_LOCAL_PRIVATE_KEY")
            .args(args)
            .output()
            .unwrap()
    };
    let first = stdout(&run(&["get-private-key", "pkp-1"]));
    let second = stdout(&run(&["get-private-key", "pkp-1"]));
    assert_eq!(first, second, "generated key must persist and be reused");
    assert!(state.join("master.key").exists());
}

#[test]
fn run_executes_a_shell_bundle_and_surfaces_response() {
    let (dir, state) = workspace();
    let d = dir.path();
    // No manifest needed: the bundle's root startup.sh is the entrypoint.
    std::fs::write(
        d.join("startup.sh"),
        "lit print \"hi\"\nKEY=$(lit get-private-key pkp-1)\nlit set-response \"len=${#KEY}\"\n",
    )
    .unwrap();

    let out = lit(d, &state, &["run"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The recorded response is printed to stdout; 0x + 64 hex = 66 chars.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), "len=66");
}

#[test]
fn run_propagates_nonzero_exit_and_withholds_response() {
    let (dir, state) = workspace();
    let d = dir.path();
    // Record a response, then fail: a failed run must NOT surface it on stdout.
    std::fs::write(
        d.join("startup.sh"),
        "lit set-response should-not-be-printed\necho boom >&2\nexit 3\n",
    )
    .unwrap();

    let out = lit(d, &state, &["run"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "stdout must be empty on failure, got: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn run_startup_script_flag_overrides_bundle_script() {
    // The --startup-script override is the local analog of the
    // request-supplied startup_script: same bundle, different script.
    let (dir, state) = workspace();
    let d = dir.path();
    std::fs::write(d.join("startup.sh"), "lit set-response from-bundle\n").unwrap();
    std::fs::write(d.join("other.sh"), "lit set-response from-override\n").unwrap();

    let out = lit(d, &state, &["run", "--startup-script", "other.sh"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "from-override"
    );
}

#[test]
fn run_without_any_startup_script_fails() {
    let (dir, state) = workspace();
    let out = lit(dir.path(), &state, &["run"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("startup.sh"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn run_injects_js_params_as_env_vars() {
    // Same injection rules as the sandbox: strings verbatim, other JSON
    // compact, reserved/invalid names skipped (PATH must stay intact or the
    // `lit` calls below could not even resolve).
    let (dir, state) = workspace();
    let d = dir.path();
    std::fs::write(
        d.join("lit.job.json"),
        r#"{"jsParams":{"name":"lit","count":2,"PATH":"/evil","not-a-name":"x"}}"#,
    )
    .unwrap();
    std::fs::write(d.join("startup.sh"), "lit set-response \"$name-$count\"\n").unwrap();

    let out = lit(d, &state, &["run"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), "lit-2");
}

#[test]
fn read_only_command_leaves_no_state_dir() {
    let (dir, state) = workspace();
    // A read-only command must not generate a master key or create .lit-local.
    assert_eq!(stdout(&lit(dir.path(), &state, &["params"])), "null");
    assert!(!state.exists(), "params must not create the state dir");
}

#[test]
fn cli_env_wiring_overrides_manifest_env() {
    // The manifest tries to set LIT_ACTION_IPFS_ID; the CLI's --ipfs-id must
    // win (manifest.env is applied first so CLI wiring can't be clobbered).
    let (dir, state) = workspace();
    let d = dir.path();
    std::fs::write(
        d.join("lit.json"),
        r#"{"env":{"LIT_ACTION_IPFS_ID":"QmFromManifest"}}"#,
    )
    .unwrap();
    std::fs::write(
        d.join("startup.sh"),
        "lit set-response \"$LIT_ACTION_IPFS_ID\"\n",
    )
    .unwrap();

    let out = lit(d, &state, &["--ipfs-id", "QmFromFlag", "run"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "QmFromFlag"
    );
}

#[test]
fn empty_plaintext_does_not_round_trip() {
    // Parity with the TEE: encrypting "" yields a ciphertext that fails to
    // decrypt rather than returning an empty string.
    let (dir, state) = workspace();
    let d = dir.path();
    let ct = stdout(&lit(d, &state, &["aes-encrypt", "pkp-1", ""]));
    assert!(
        !lit(d, &state, &["aes-decrypt", "pkp-1", &ct])
            .status
            .success()
    );
}

#[test]
fn run_discovers_job_next_to_the_bundle() {
    // `lit run` from a parent dir must read the bundle's own lit.job.json,
    // not one in the caller's CWD, and export its CID to the action.
    let (dir, state) = workspace();
    let d = dir.path();
    let bundle = d.join("bundle");
    std::fs::create_dir(&bundle).unwrap();
    std::fs::write(
        bundle.join("lit.job.json"),
        r#"{"ipfsId":"QmBundleCid","jsParams":{"from":"bundle"}}"#,
    )
    .unwrap();
    // A decoy job file in the caller's CWD that must be ignored.
    std::fs::write(
        d.join("lit.job.json"),
        r#"{"ipfsId":"QmWrong","jsParams":{"from":"cwd"}}"#,
    )
    .unwrap();
    std::fs::write(
        bundle.join("startup.sh"),
        "lit set-response \"$(lit params)|$LIT_ACTION_IPFS_ID\"\n",
    )
    .unwrap();

    let out = lit(d, &state, &["run", "--dir", "bundle"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        r#"{"from":"bundle"}|QmBundleCid"#
    );
}
