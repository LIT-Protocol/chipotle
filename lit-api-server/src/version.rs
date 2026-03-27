use git_version::git_submodule_versions;
use git_version::git_version;

pub(crate) const GIT_VERSION: &str = git_version!();
pub(crate) const GIT_SUBMODULE_VERSIONS: &[(&str, &str)] = &git_submodule_versions!();
pub(crate) const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
pub(crate) const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
