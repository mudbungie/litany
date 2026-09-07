//! The continuation-address grammar (ARCH §3.3 *Paging a cut capture*).
//!
//! [`super::run`]'s own tests drive every rejection through the tool, in
//! the voice the model reads. Here the subject is the round trip: what
//! the cut marker mints is what a read parses, and what a read hands
//! back for the next page is the same spelling again — the one property
//! that keeps the marker and the tool from disagreeing about a string
//! neither of them stores.

use super::address::{Address, mint};
use std::path::Path;

const RECORD: &str = "steps/root-child/007/tools/toolu_01abc/output.json";

#[test]
fn a_minted_address_parses_back_to_what_was_minted() {
    let minted = mint(Path::new(RECORD), "stdout", 2048);
    assert_eq!(minted, format!("{RECORD}#stdout@2048"));
    let parsed = Address::parse(&minted).expect("the mint is the grammar");
    assert_eq!(parsed.record(), RECORD);
    assert_eq!(parsed.agent_id, "root-child");
    assert_eq!(parsed.stream, "stdout");
    assert_eq!(parsed.offset, 2048);
}

/// The next page's address is the same address moved — re-rendered
/// through [`mint`], so a page never spells the grammar its own way.
#[test]
fn the_next_page_is_the_same_address_moved() {
    let parsed = Address::parse(&format!("{RECORD}#stderr@0")).expect("grammar");
    assert_eq!(parsed.at(4096), format!("{RECORD}#stderr@4096"));
}
