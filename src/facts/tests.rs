//! The write-side cap (ARCH §5.5): a config commit's facts file is
//! refused when it is over [`MAX_BYTES`], because a pinned path is
//! never shed at assembly.

use super::{FILE, MAX_BYTES, OverCap, require_within_cap};
use tempfile::TempDir;

/// Write a facts file of exactly `bytes` bytes into a fresh checkout.
fn checkout_with(bytes: u64) -> TempDir {
    let dir = TempDir::new().unwrap();
    let filler = "x".repeat(usize::try_from(bytes).unwrap());
    std::fs::write(dir.path().join(FILE), filler).unwrap();
    dir
}

#[test]
fn a_file_at_the_cap_is_accepted() {
    // The boundary is inclusive: 4096 bytes is the artifact's whole
    // allowance, not one byte short of it.
    let dir = checkout_with(MAX_BYTES);
    require_within_cap(dir.path()).unwrap();
}

#[test]
fn one_byte_over_is_declined_naming_both_numbers() {
    let dir = checkout_with(MAX_BYTES + 1);
    let err = require_within_cap(dir.path()).unwrap_err();
    assert!(
        matches!(err, OverCap::TooLarge { bytes } if bytes == MAX_BYTES + 1),
        "{err:?}"
    );
    // "Over capacity" without the two numbers leaves the author
    // guessing at how much to cut.
    let text = err.to_string();
    assert!(text.contains(&(MAX_BYTES + 1).to_string()), "{text}");
    assert!(text.contains(&MAX_BYTES.to_string()), "{text}");
    assert!(text.contains(FILE), "{text}");
}

#[test]
fn no_facts_file_is_the_general_path_with_empty_inputs() {
    let dir = TempDir::new().unwrap();
    require_within_cap(dir.path()).unwrap();
}

#[test]
fn a_file_that_cannot_be_measured_surfaces_rather_than_reading_as_absent() {
    // A checkout path that is itself a file: the metadata read fails
    // with something other than NotFound, and a ceiling that answered
    // "fine" here would not be a ceiling.
    let dir = TempDir::new().unwrap();
    let not_a_dir = dir.path().join("checkout");
    std::fs::write(&not_a_dir, b"x").unwrap();
    let err = require_within_cap(&not_a_dir).unwrap_err();
    assert!(matches!(err, OverCap::Io(_)), "{err:?}");
    assert!(err.to_string().contains(FILE), "{err}");
}
