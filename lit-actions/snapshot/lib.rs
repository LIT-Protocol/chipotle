use deno_runtime::{deno_core::Extension, snapshot};

pub use deno_runtime::snapshot::{CreateRuntimeSnapshotOutput, LazyExtensionFileKind};

pub fn create_snapshot(
    snapshot_path: std::path::PathBuf,
    custom_exts: Vec<Extension>,
) -> CreateRuntimeSnapshotOutput {
    snapshot::create_runtime_snapshot(snapshot_path, Default::default(), custom_exts)
}
