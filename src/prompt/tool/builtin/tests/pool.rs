//! The advertised built-in pool (ARCH §3.3): the one list behind both
//! the unknown-tool decline and `litany tool --help`, and the decline's
//! rendering of it.

use super::super::{NAMES, pool};
use super::route;
use std::io::Cursor;

#[test]
fn unknown_tool_name_surfaces_unknown_variant() {
    let mut stdin = Cursor::new(Vec::<u8>::new());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = route("not_a_tool", &mut stdin, &mut stdout, &mut stderr).unwrap_err();
    // The decline names the available pool, in the same voice `load_skill`
    // declines an unknown skill with (§3.3) — the CLI's only chance to tell
    // an operator or a model what it *could* have said.
    assert_eq!(
        err.to_string(),
        "unknown built-in tool: \"not_a_tool\"; available: \
         apply_patch, bash, cd, dispatch, load_skill, message, read_file"
    );
}

/// The advertised pool is exactly the arms `run_with` routes for a
/// general agent — sorted, and without the compactor pair, which §2.7
/// injects for the compactor role alone and no one elects by name.
#[test]
fn pool_is_the_sorted_advertised_name_set() {
    assert_eq!(
        pool(),
        "apply_patch, bash, cd, dispatch, load_skill, message, read_file"
    );
    let mut sorted = NAMES;
    sorted.sort_unstable();
    assert_eq!(NAMES, sorted);
}
