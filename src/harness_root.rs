//! Resolution of the **harness root** (ARCH §2.2), split along XDG
//! lifetimes into a **config root** and a **data root**.
//!
//! - **Config root** (`$XDG_CONFIG_HOME/litany`, default
//!   `~/.config/litany`) holds the hand-edited declarations: the global
//!   `models.yaml` (ARCH §4.2) and the `workflows/` templates.
//! - **Data root** (`$XDG_DATA_HOME/litany`, default
//!   `~/.local/share/litany`) holds machine-populated state: the
//!   `workspaces/` tree and the `skills/` and `tools/`
//!   pools the harness copies from at conversation bootstrap.
//!
//! `LITANY_HOME`, when set and non-empty, is the single override that
//! **collapses both roots to that one directory** — test isolation
//! (parallel tests, sandboxed replay) keeps working with one env var.
//! Otherwise each root resolves XDG-style, matching brazen's own
//! resolution. An empty override or empty XDG var falls through to the
//! home-based default — an empty env var would otherwise produce the
//! current working directory's neighbor and is almost never intended.
//!
//! Resolution is deliberately not cached — tests scope env-var changes
//! per call, and the cost of a few `getenv`s per use is irrelevant next
//! to the I/O it gates.

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use thiserror::Error;

const ENV_HOME: &str = "LITANY_HOME";
const ENV_XDG_CONFIG: &str = "XDG_CONFIG_HOME";
const ENV_XDG_DATA: &str = "XDG_DATA_HOME";
const SUBDIR: &str = "litany";
const CONFIG_FALLBACK: &str = ".config";
const DATA_FALLBACK: &str = ".local/share";
const MODELS_FILE: &str = "models.yaml";

/// Why [`resolve`] could not produce paths. The only failure is the
/// "no override, no XDG var, and no home" triple — every other case
/// yields paths (whether or not the directories exist on disk).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("LITANY_HOME is unset and no home directory is available")]
    NoHome,
}

/// The harness root, split by XDG lifetime. Under `LITANY_HOME` both
/// fields hold the same directory; under XDG they diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roots {
    /// Config-lifetime root: `models.yaml`, `workflows/`.
    pub config: PathBuf,
    /// Data-lifetime root: `workspaces/`, `skills/`, `tools/`.
    pub data: PathBuf,
}

/// Pure resolver. `override_value` is the literal `LITANY_HOME` (or
/// `None`); `xdg_config` / `xdg_data` are the literal `XDG_CONFIG_HOME`
/// / `XDG_DATA_HOME` values; `home` is the user's home directory. A
/// non-empty override collapses both roots to it; otherwise each root
/// is `<xdg-or-home-fallback>/litany`.
pub fn resolve_from(
    override_value: Option<&OsStr>,
    xdg_config: Option<&OsStr>,
    xdg_data: Option<&OsStr>,
    home: Option<&Path>,
) -> Result<Roots, Error> {
    if let Some(v) = override_value
        && !v.is_empty()
    {
        let both = PathBuf::from(v);
        return Ok(Roots {
            config: both.clone(),
            data: both,
        });
    }
    Ok(Roots {
        config: xdg_root(xdg_config, home, CONFIG_FALLBACK)?,
        data: xdg_root(xdg_data, home, DATA_FALLBACK)?,
    })
}

/// One XDG-style root: `$XDG_.../litany` when the var is set and
/// non-empty, else `<home>/<fallback>/litany`.
fn xdg_root(xdg: Option<&OsStr>, home: Option<&Path>, fallback: &str) -> Result<PathBuf, Error> {
    if let Some(v) = xdg
        && !v.is_empty()
    {
        return Ok(Path::new(v).join(SUBDIR));
    }
    home.map(|h| h.join(fallback).join(SUBDIR))
        .ok_or(Error::NoHome)
}

/// Resolve the split harness root from the live process environment.
pub fn resolve() -> Result<Roots, Error> {
    let override_value = env::var_os(ENV_HOME);
    let xdg_config = env::var_os(ENV_XDG_CONFIG);
    let xdg_data = env::var_os(ENV_XDG_DATA);
    #[allow(deprecated)] // un-deprecated in Rust 1.86; lint precedes that.
    let home = env::home_dir();
    resolve_from(
        override_value.as_deref(),
        xdg_config.as_deref(),
        xdg_data.as_deref(),
        home.as_deref(),
    )
}

/// Path to the global `models.yaml` within the config root (ARCH §4.2).
pub fn models_path(config_root: &Path) -> PathBuf {
    config_root.join(MODELS_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn override_collapses_both_roots() {
        let r = resolve_from(
            Some(OsStr::new("/opt/litany")),
            Some(OsStr::new("/xc")),
            Some(OsStr::new("/xd")),
            Some(Path::new("/home/x")),
        )
        .unwrap();
        assert_eq!(r.config, PathBuf::from("/opt/litany"));
        assert_eq!(r.data, PathBuf::from("/opt/litany"));
    }

    #[test]
    fn empty_override_falls_through_to_xdg() {
        let r = resolve_from(
            Some(OsStr::new("")),
            Some(OsStr::new("/xc")),
            Some(OsStr::new("/xd")),
            Some(Path::new("/home/x")),
        )
        .unwrap();
        assert_eq!(r.config, PathBuf::from("/xc/litany"));
        assert_eq!(r.data, PathBuf::from("/xd/litany"));
    }

    #[test]
    fn xdg_vars_name_each_root() {
        let r = resolve_from(None, Some(OsStr::new("/xc")), Some(OsStr::new("/xd")), None).unwrap();
        assert_eq!(r.config, PathBuf::from("/xc/litany"));
        assert_eq!(r.data, PathBuf::from("/xd/litany"));
    }

    #[test]
    fn unset_xdg_uses_home_fallbacks() {
        let r = resolve_from(None, None, None, Some(Path::new("/home/x"))).unwrap();
        assert_eq!(r.config, PathBuf::from("/home/x/.config/litany"));
        assert_eq!(r.data, PathBuf::from("/home/x/.local/share/litany"));
    }

    #[test]
    fn empty_xdg_var_falls_through_to_home_fallback() {
        let r = resolve_from(
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("")),
            Some(Path::new("/home/x")),
        )
        .unwrap();
        assert_eq!(r.config, PathBuf::from("/home/x/.config/litany"));
        assert_eq!(r.data, PathBuf::from("/home/x/.local/share/litany"));
    }

    #[test]
    fn missing_config_root_is_an_error() {
        // No override, no XDG_CONFIG_HOME, no home → config leg fails.
        let err = resolve_from(None, None, Some(OsStr::new("/xd")), None).unwrap_err();
        assert_eq!(err, Error::NoHome);
    }

    #[test]
    fn missing_data_root_is_an_error() {
        // Config leg succeeds via XDG_CONFIG_HOME; data leg has neither
        // XDG_DATA_HOME nor a home directory to fall back on.
        let err = resolve_from(None, Some(OsStr::new("/xc")), None, None).unwrap_err();
        assert_eq!(err, Error::NoHome);
    }

    #[test]
    fn empty_override_with_no_home_and_no_xdg_is_an_error() {
        let err = resolve_from(Some(OsStr::new("")), None, None, None).unwrap_err();
        assert_eq!(err, Error::NoHome);
    }

    #[test]
    fn override_value_with_path_separator_is_preserved() {
        let v = os("/srv/data/litany");
        let r = resolve_from(Some(v.as_os_str()), None, None, None).unwrap();
        assert_eq!(r.config, PathBuf::from("/srv/data/litany"));
        assert_eq!(r.data, PathBuf::from("/srv/data/litany"));
    }

    #[test]
    fn models_path_appends_filename() {
        assert_eq!(
            models_path(Path::new("/xc/litany")),
            PathBuf::from("/xc/litany/models.yaml")
        );
    }

    #[test]
    fn live_resolve_returns_some_paths() {
        // The live resolver must produce *something* on this host: the
        // test harness has either LITANY_HOME set or a home directory.
        // Asserting only that it succeeds keeps the test independent of
        // the runner's environment.
        let _ = resolve().expect("either LITANY_HOME or HOME must be set");
    }
}
