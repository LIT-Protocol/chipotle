use git_version::git_version;

pub(crate) const GIT_VERSION: &str = git_version!();
// `git_submodule_versions!()` from git-version 0.3.9 mis-joins `$displaypath`
// (relative to CARGO_MANIFEST_DIR) onto the repo root, breaking the build whenever
// the crate lives in a subdirectory and the repo has submodules. The only submodule
// here is the Foundry `forge-std` test dependency, which is irrelevant at runtime.
pub(crate) const GIT_SUBMODULE_VERSIONS: &[(&str, &str)] = &[];
pub(crate) const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
pub(crate) const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
