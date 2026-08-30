//! `<conv-repo>/version` — the schema version of a conversation repo.
//!
//! Per ARCH §10, this is a bare integer. The file's content is the integer
//! and nothing else (trailing whitespace tolerated).

use crate::config::error::LoadError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The highest schema version this harness understands (ARCH §10). A
/// config commit declaring a higher one was authored by a newer harness:
/// its control files may carry shapes this build cannot read, so it is
/// declined loudly (`docs/PRINCIPLES.md` "Decline illegal operations")
/// rather than read on a guess. Older versions stay readable — that is
/// the §10 promise, and what migration code is written for on a bump.
pub const SUPPORTED: u32 = 1;

/// The schema version of a conversation repo's config, already checked
/// against [`SUPPORTED`]. Constructing one *is* the guard — there is no
/// separate `check` step a caller can forget, and no representable
/// `Version` this build cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Version(pub u32);

impl Version {
    /// Parse the `version` control file's content already in hand — the
    /// governing-config read path (ARCH §2.2: control is read from the
    /// config commit's tree, never from a worktree file). `origin` labels
    /// errors (e.g. `<config-commit>:version`). The file's content is the
    /// integer and nothing else (trailing whitespace tolerated), and a
    /// version above [`SUPPORTED`] is declined.
    pub fn parse(raw: &str, origin: &Path) -> Result<Self, LoadError> {
        let trimmed = raw.trim();
        let parsed: u32 = trimmed.parse().map_err(|_| LoadError::Invalid {
            path: origin.to_path_buf(),
            key: ".".into(),
            message: format!("expected an unsigned integer, got {trimmed:?}"),
        })?;
        if parsed > SUPPORTED {
            return Err(LoadError::Invalid {
                path: origin.to_path_buf(),
                key: ".".into(),
                message: format!(
                    "schema version {parsed} is newer than this harness understands \
                     (supported: {SUPPORTED}); upgrade litany to read this config"
                ),
            });
        }
        Ok(Version(parsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> &'static Path {
        Path::new("<commit>:version")
    }

    #[test]
    fn parses_a_bare_integer() {
        assert_eq!(Version::parse("1\n", origin()).unwrap(), Version(SUPPORTED));
    }

    #[test]
    fn tolerates_trailing_whitespace() {
        assert_eq!(Version::parse("  1  \n\n", origin()).unwrap(), Version(1));
    }

    #[test]
    fn rejects_non_integer_content() {
        let err = Version::parse("v1\n", origin()).unwrap_err();
        match err {
            LoadError::Invalid { message, .. } => {
                assert!(message.contains("\"v1\""), "got: {message}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn declines_a_version_newer_than_this_harness() {
        // ARCH §10: old versions stay readable, newer ones are declined
        // loudly rather than read on a guess.
        let err = Version::parse(&format!("{}\n", SUPPORTED + 1), origin()).unwrap_err();
        match err {
            LoadError::Invalid { path, message, .. } => {
                assert_eq!(path, origin());
                assert!(message.contains(&(SUPPORTED + 1).to_string()), "{message}");
                assert!(message.contains("upgrade litany"), "{message}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
