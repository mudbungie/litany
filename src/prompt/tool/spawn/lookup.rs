//! The §3.3 second resolution hop: `litany-tool-<name>` on `PATH`.
//!
//! Split from [`super`] so the lookup — a trait, its production
//! implementation, and the two functions behind it — sits apart from the
//! executor that consults it, and so the executor file stays under the
//! repo's 300-line cap.

use std::ffi::OsStr;
use std::path::PathBuf;

/// Indirection for the §3.3 second hop so tests can drive the PATH
/// lookup without manipulating the process env. Production wires
/// [`EnvPath`], which reads the live `PATH`. The third hop needs no
/// indirection: its target is injected, not looked up.
pub trait PathLookup {
    /// PATH lookup for the externalized tool binary
    /// (`litany-tool-<name>`), the second hop in §3.3 resolution.
    fn which_on_path(&self, prefixed_name: &str) -> Option<PathBuf>;
}

/// Real-process lookup: the live `PATH`, via [`which_in_path`].
pub struct EnvPath;

impl PathLookup for EnvPath {
    fn which_on_path(&self, prefixed_name: &str) -> Option<PathBuf> {
        which_in_path(prefixed_name)
    }
}

/// PATH lookup for `name` against the live process env. Wraps
/// [`which_in_path_env`] so the env-var read sits in one place; tests
/// drive `which_in_path_env` directly with a constructed path and
/// invoke this wrapper once for the env-read branch.
pub(crate) fn which_in_path(name: &str) -> Option<PathBuf> {
    which_in_path_env(name, std::env::var_os("PATH").as_deref())
}

/// PATH lookup that takes the path string as a parameter. First hit
/// wins. Returns an absolute path so the spawn is unambiguous.
pub(crate) fn which_in_path_env(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let path = path?;
    for dir in std::env::split_paths(path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
