//! The brazen pin's one reader (ARCH §4.4 "Version skew is guarded").
//!
//! The pin has a single home — the `brazen = "=<pin>"` dependency in
//! `Cargo.toml` — and every consumer derives from it rather than
//! mirroring the number: the load-time version guard
//! ([`super::resolve`]), the not-found refusal's fix-it command
//! ([`super::Error::AdapterMissing`]), `litany --version`, and the
//! Makefile's `BRAZEN_PIN` (which reads the same line with `sed`, and
//! is pinned to agree by `super::tests::pin`).

/// The crate manifest, embedded so [`brazen_pin`] derives from the
/// pin's one home (`Cargo.toml`) instead of mirroring it.
const MANIFEST: &str = include_str!("../../Cargo.toml");

/// The exact brazen pin in a manifest — the version inside
/// `brazen = "=<pin>"`, in either the inline spelling of the source
/// `Cargo.toml` or the `[dependencies.brazen]` / `version = "=<pin>"`
/// table spelling cargo normalizes published manifests into. `None`
/// when the manifest carries no exact brazen pin.
pub(super) fn parse_brazen_pin(manifest: &str) -> Option<&str> {
    let mut in_brazen_table = false;
    for line in manifest.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("brazen = \"=") {
            return rest.strip_suffix('"');
        }
        if line.starts_with('[') {
            in_brazen_table = line == "[dependencies.brazen]";
        } else if in_brazen_table && let Some(rest) = line.strip_prefix("version = \"=") {
            return rest.strip_suffix('"');
        }
    }
    None
}

/// The exact brazen crate version litany links, read from the
/// `brazen = "=<pin>"` dependency in the embedded `Cargo.toml` — the
/// number's one home (the `make install` pin derives from the same
/// line). The load-time version guard rejects a `bz` whose `--version`
/// differs (§4.4 "Version skew is guarded").
pub fn brazen_pin() -> &'static str {
    parse_brazen_pin(MANIFEST).expect("Cargo.toml pins brazen as `brazen = \"=<version>\"`")
}
