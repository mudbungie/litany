//! `<harness-root>/models.yaml` — the optional `adapter:` binary
//! override (ARCH §4.2, §4.4 Extensibility). Mechanism only.
//!
//! The file once carried a `models:` table (per-model capabilities and
//! context windows) that nothing in the harness acted on and that
//! shipped model policy in git — the bl-3157 class of bug: a hand-typed
//! model id validated only against another hand-typed line. bl-35e2
//! retired the table: a role's `providers.yaml` assignment (§4.3) is the
//! single home of the (provider row, model id) pointer, id validity is
//! brazen's fact caught at the first live model call (§4.2), and this
//! file carries only what remains litany's — which adapter binary to
//! run. A leftover `models:` block in an operator's file is ignored on
//! parse (serde's default for unknown keys), so existing installs load
//! unchanged.

use crate::config::error::LoadError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level shape of the harness-root `models.yaml` (ARCH §4.2).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct Models {
    /// Optional provider-adapter binary override (§4.2, §4.4). Default
    /// (`None`) resolves `bz` on `PATH`. Any binary honoring the same
    /// pipe contract slots in verbatim; the load-time version guard is
    /// skipped under an override and the in-band `MessageStart.v`
    /// handshake governs instead (§4.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<PathBuf>,
}

impl Models {
    /// Read and parse `models.yaml` at `path`. A comments-only or empty
    /// file (the shipped template names nothing) parses as the default —
    /// no override, `bz` on `PATH`.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let raw = fs::read_to_string(path).map_err(|source| LoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let parsed: Option<Self> =
            serde_yaml_ng::from_str(&raw).map_err(|source| LoadError::Yaml {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(parsed.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_yaml(s: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(s.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_adapter_override() {
        let f = write_yaml("adapter: /usr/local/bin/bz\n");
        let m = Models::load(f.path()).unwrap();
        assert_eq!(m.adapter.as_deref(), Some(Path::new("/usr/local/bin/bz")));
    }

    #[test]
    fn comments_only_file_parses_as_default() {
        // The shipped template is comments-only (bl-35e2: git carries no
        // model policy), which YAML parses as null — the default shape.
        let f = write_yaml("# no override; `bz` on PATH governs\n");
        let m = Models::load(f.path()).unwrap();
        assert!(m.adapter.is_none());
    }

    #[test]
    fn retired_models_table_is_ignored() {
        // Pre-bl-35e2 files (and yog's picker-written entries) carry a
        // `models:` block; it is inert, never a parse error.
        let f = write_yaml(
            "models:\n  m:\n    provider: p\n    model_id: m\n    \
             capabilities: []\n    context_window: 1\n",
        );
        let m = Models::load(f.path()).unwrap();
        assert!(m.adapter.is_none());
    }

    #[test]
    fn surfaces_yaml_parse_errors() {
        let f = write_yaml("not: [valid: yaml");
        let err = Models::load(f.path()).unwrap_err();
        assert!(matches!(err, LoadError::Yaml { .. }));
    }

    #[test]
    fn surfaces_io_errors() {
        let err = Models::load(Path::new("/no/such/models.yaml")).unwrap_err();
        assert!(matches!(err, LoadError::Io { .. }));
    }
}
