use std::collections::HashSet;
use std::fmt::Write as _;

fn main() {
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    // Create V8 snapshot from Deno runtime + Lit Actions extension.
    let snapshot_output = lit_actions_snapshot::create_snapshot(
        out_dir.join("RUNTIME_SNAPSHOT.bin"),
        vec![lit_actions_ext::lit_actions::init()],
    );

    // Newer deno_core leaves some lazy extension sources out of the startup
    // snapshot and expects embedders to provide the residual source table at
    // runtime. Generate a tiny Rust include file so fetch/web APIs can lazy-load
    // their backing scripts after bootstrapping from our custom snapshot.
    let consumed: HashSet<_> = snapshot_output
        .consumed_lazy_specifiers
        .into_iter()
        .collect();
    let mut lazy_js = Vec::new();
    let mut lazy_esm = Vec::new();

    for file in snapshot_output.lazy_extension_files {
        if consumed.contains(&file.specifier) {
            continue;
        }
        println!("cargo:rerun-if-changed={}", file.path.display());
        let source = std::fs::read_to_string(&file.path).unwrap_or_else(|err| {
            panic!(
                "failed to read lazy extension file {}: {err}",
                file.path.display()
            )
        });
        match file.kind {
            lit_actions_snapshot::LazyExtensionFileKind::Js => {
                lazy_js.push((file.specifier, source))
            }
            lit_actions_snapshot::LazyExtensionFileKind::Esm => {
                lazy_esm.push((file.specifier, source))
            }
        }
    }

    fn write_pairs(out: &mut String, name: &str, pairs: &[(String, String)]) {
        let _ = writeln!(out, "const {name}: &[(&str, &str)] = &[");
        for (specifier, source) in pairs {
            let _ = writeln!(out, "    ({specifier:?}, {source:?}),");
        }
        let _ = writeln!(out, "];\n");
    }

    let mut generated = String::new();
    write_pairs(&mut generated, "RESIDUAL_LAZY_JS_SOURCES", &lazy_js);
    write_pairs(&mut generated, "RESIDUAL_LAZY_ESM_SOURCES", &lazy_esm);

    std::fs::write(out_dir.join("runtime_lazy_sources.rs"), generated)
        .expect("failed to write runtime_lazy_sources.rs");
}
