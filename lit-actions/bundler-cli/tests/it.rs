//! End-to-end tests driving the real `lit-bundle` binary. `deploy`/`run` need
//! a live node, so these focus on `bundle` — the part with all the local logic
//! (packaging, startup.sh generation, content-addressed CIDs).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// Run `lit-bundle <args>`.
fn lit_bundle(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lit-bundle"))
        .args(args)
        .output()
        .expect("failed to run lit-bundle")
}

fn ok_stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn write(dir: &Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).unwrap();
}

/// Unpack a tar/tar.gz bundle into `{path -> (mode, contents)}`.
fn unpack(bytes: &[u8]) -> BTreeMap<String, (u32, String)> {
    let reader: Box<dyn Read> = if bytes.starts_with(&[0x1f, 0x8b]) {
        Box::new(flate2::read::GzDecoder::new(bytes))
    } else {
        Box::new(bytes)
    };
    let mut archive = tar::Archive::new(reader);
    let mut out = BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let mode = entry.header().mode().unwrap();
        let path = entry.path().unwrap().to_string_lossy().into_owned();
        let mut contents = String::new();
        entry.read_to_string(&mut contents).unwrap();
        out.insert(path, (mode, contents));
    }
    out
}

/// `bundle <dir> -o <out>` and return (cid, bundle bytes).
fn bundle(dir: &Path, extra: &[&str]) -> (String, Vec<u8>) {
    let out = dir.join("__bundle.out");
    let mut args = vec!["bundle", dir.to_str().unwrap(), "-o", out.to_str().unwrap()];
    args.extend_from_slice(extra);
    let cid = ok_stdout(&lit_bundle(&args));
    let bytes = std::fs::read(&out).unwrap();
    // Don't let our own output pollute a re-bundle of the same dir.
    std::fs::remove_file(&out).unwrap();
    (cid, bytes)
}

fn project() -> TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn bundles_a_folder_with_startup_and_emits_a_qm_cid() {
    let d = project();
    write(d.path(), "startup.sh", "lit set-response '{\"ok\":true}'\n");
    write(d.path(), "data.txt", "hello");

    let (cid, bytes) = bundle(d.path(), &[]);

    assert!(cid.starts_with("Qm"), "expected an IPFS CIDv0: {cid}");
    let files = unpack(&bytes);
    assert!(files.contains_key("startup.sh"));
    assert_eq!(files["data.txt"].1, "hello");
}

#[test]
fn cid_is_deterministic_across_builds() {
    let d = project();
    write(d.path(), "startup.sh", "echo hi\n");
    write(d.path(), "a.txt", "aaa");
    write(d.path(), "b.txt", "bbb");

    let (cid1, bytes1) = bundle(d.path(), &[]);
    let (cid2, bytes2) = bundle(d.path(), &[]);

    assert_eq!(cid1, cid2, "same inputs must yield the same CID");
    assert_eq!(bytes1, bytes2, "same inputs must yield identical bytes");
}

#[test]
fn generates_startup_from_binary_and_marks_it_executable() {
    let d = project();
    write(d.path(), "app", "#!/bin/sh\necho run\n");

    let out = lit_bundle(&[
        "bundle",
        d.path().to_str().unwrap(),
        "-o",
        d.path().join("out.tgz").to_str().unwrap(),
        "--binary",
        "app",
    ]);
    let cid = ok_stdout(&out);
    assert!(cid.starts_with("Qm"));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("generated startup.sh"),
        "should report the generated startup.sh"
    );

    let bytes = std::fs::read(d.path().join("out.tgz")).unwrap();
    let files = unpack(&bytes);
    let startup = &files["startup.sh"].1;
    assert!(
        startup.contains("exec ./app"),
        "startup runs the binary: {startup}"
    );
    // The binary must carry an exec bit in the read-only sandbox mount.
    assert_eq!(files["app"].0 & 0o111, 0o111, "binary must be executable");
}

#[test]
fn no_startup_and_no_binary_is_an_error() {
    let d = project();
    write(d.path(), "notes.txt", "no entrypoint here");

    let out = lit_bundle(&["bundle", d.path().to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--binary"),
        "error should mention --binary: {err}"
    );
}

#[test]
fn binary_naming_a_missing_file_is_an_error() {
    let d = project();
    write(d.path(), "startup.sh", "echo hi\n");

    // Write output into the tempdir so the run never pollutes the test CWD.
    let out = lit_bundle(&[
        "bundle",
        d.path().to_str().unwrap(),
        "-o",
        d.path().join("out.tgz").to_str().unwrap(),
        "--binary",
        "nope",
    ]);
    // startup.sh already exists, so --binary is unused; but if we force it via
    // a folder without startup.sh, a missing binary must fail. Cover that:
    let d2 = project();
    write(d2.path(), "other", "x");
    let out2 = lit_bundle(&[
        "bundle",
        d2.path().to_str().unwrap(),
        "-o",
        d2.path().join("out.tgz").to_str().unwrap(),
        "--binary",
        "nope",
    ]);
    // First case succeeds (startup.sh wins); second fails (missing binary).
    assert!(out.status.success());
    assert!(!out2.status.success());
}

#[test]
fn config_overrides_folder_manifest_and_must_be_valid_json() {
    let d = project();
    write(d.path(), "startup.sh", "echo hi\n");
    write(d.path(), "lit.json", "{\"runtime\":\"folder\"}");

    // A separate, valid config replaces the folder's lit.json.
    let cfg = project();
    write(
        cfg.path(),
        "cfg.json",
        "{\"runtime\":\"python3\",\"env\":{\"X\":\"1\"}}",
    );
    let (_cid, bytes) = bundle(
        d.path(),
        &["--config", cfg.path().join("cfg.json").to_str().unwrap()],
    );
    let files = unpack(&bytes);
    assert!(files["lit.json"].1.contains("python3"));
    assert!(!files["lit.json"].1.contains("folder"));

    // Invalid JSON is rejected up front.
    write(cfg.path(), "bad.json", "{not json");
    let out = lit_bundle(&[
        "bundle",
        d.path().to_str().unwrap(),
        "-o",
        d.path().join("x.tgz").to_str().unwrap(),
        "--config",
        cfg.path().join("bad.json").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
}

#[test]
fn plain_tar_and_gzip_hash_differently_but_each_is_stable() {
    let d = project();
    write(d.path(), "startup.sh", "echo hi\n");

    let (gz_cid, gz_bytes) = bundle(d.path(), &[]);
    let (tar_cid, tar_bytes) = bundle(d.path(), &["--no-compress"]);

    assert!(gz_bytes.starts_with(&[0x1f, 0x8b]), "default is gzip");
    assert!(
        !tar_bytes.starts_with(&[0x1f, 0x8b]),
        "--no-compress is plain tar"
    );
    assert_ne!(gz_cid, tar_cid);
    // Both still unpack to the same logical contents.
    assert_eq!(
        unpack(&gz_bytes).keys().collect::<Vec<_>>(),
        unpack(&tar_bytes).keys().collect::<Vec<_>>()
    );
}
