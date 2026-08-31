//! The operator-notice prefix is a published contract (ARCH §2.11), so
//! it is asserted literally: a consumer reading `driver.log` keys on
//! these exact bytes, and a reworded prefix is a broken consumer, not a
//! cosmetic edit.

use super::{PREFIX, line};

#[test]
fn prefix_is_the_published_bytes() {
    assert_eq!(PREFIX, "litany: notice: ");
}

#[test]
fn line_prefixes_the_formatted_body() {
    assert_eq!(
        line(format_args!("budget {} on {}", "depth", "agents/a-b")),
        "litany: notice: budget depth on agents/a-b"
    );
}

#[test]
fn line_is_a_bare_concatenation() {
    // No spacing, quoting or punctuation is composed here — the prefix
    // carries its own separator and the body is verbatim, so a site's
    // sentence reads on the wire exactly as it reads in the source.
    assert_eq!(line(format_args!("")), PREFIX);
}
