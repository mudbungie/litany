//! Folding brazen's settled in-band failure into [`super::Error`]
//! (ARCH §4.4) — the enum's one constructor that is not a `From`.
//!
//! Split from [`super`] because it is a different kind of statement: the
//! enum above is litany's failure *taxonomy*, this is the one place a
//! foreign vocabulary is translated into it, and the choice it makes
//! belongs beside the two wordings it chooses between.

use super::Error;

impl Error {
    /// Fold a settled in-band `CanonicalError` into this taxonomy,
    /// naming the **provider row** `bz` was invoked with (§4.3). The
    /// choice between the two adapter variants belongs here, beside the
    /// wordings it picks between, not at the retry loop that happens to
    /// hold the row: brazen's `auth` kind (its normalization of a
    /// 401/403) takes the remedy-bearing variant, everything else keeps
    /// the classification.
    pub(in crate::prompt) fn from_adapter(row: &str, err: brazen::CanonicalError) -> Self {
        if err.kind == brazen::ErrorKind::Auth {
            return Error::AdapterAuth {
                row: row.to_string(),
                message: err.message,
            };
        }
        Error::AdapterError {
            kind: format!("{:?}", err.kind),
            row: row.to_string(),
            message: err.message,
        }
    }
}
