//! The agent-name **mint** (ARCH §2.3): **two words from the embedded
//! wordlist joined in PascalCase** (`PeachHollow`), drawn whenever a
//! creation path omits `name`.
//!
//! Moved here from yog `src/names` by the yog bl-aca4 ruling: the moment
//! *every* creation path must mint on omission — the `dispatch` tool,
//! `litany dispatch`, `litany prompt`, none of which pass through yog —
//! the mint's one home is beside the uniqueness check it races
//! ([`super::require_available`]). Yog draws the same function through
//! the crate it already links (the [`crate::mint`] facade), so preview
//! and spawn cannot drift into two lists.
//!
//! **Two words, because one does not read as a name** (bl-79a2, operator
//! ruling 2026-08-16). A lone common noun in a conversation row or a
//! `litany list` line reads as a word that happens to be there; the
//! PascalCase pair carries the naming intent in its own shape. The join
//! has no separator, so the name stays one path component and one
//! unbroken token — and a name carrying no hyphen can never be misread
//! as two segments of the hyphenated descent `<a>-<b>-…` (§2.3).
//!
//! [`mint`] is a **pure function over an injected RNG and an occupied
//! set**: one RNG draw picks a start index into the *pair* space, then
//! the scan walks forward with wraparound, discarding each occupied name
//! for the next, to the first unoccupied one. Collision retry is that
//! scan; its bound is the pair space — exhaustion is the scan running the
//! whole pool out ([`MintError::Exhausted`], loud, never a loop). The
//! occupied set is the caller's; at the creation pre-flights it is the
//! same living-names scan ([`super::named`]) the supplied-name check
//! reads — one derivation, never a second registry.
//!
//! The pair space is the **index space widened**, not a second draw: an
//! index into `0..n * (n - 1)` names an ordered pair of distinct words,
//! so the scan, the purity, the one-draw-per-mint property and the exact
//! exhaustion bound all survive the change untouched. Consequently the
//! walk is **not uniform over pairs** — a collision steps to the next
//! *second* word, not to a fresh random pair. That is exactly what the
//! one-word mint already did, and nothing asks the mint for randomness
//! guarantees: it is a collision-avoidance device, and uniqueness is the
//! occupied-set check's, not the generator's.
//!
//! **Case is not a distinction a filesystem keeps.** The occupied set is
//! an exact-match `HashSet<String>`, so `PeachHollow` and `peachhollow`
//! are two names here and one directory on macOS or Windows. The mint
//! spells PascalCase and nothing else, so it can never produce that pair
//! itself; an operator-supplied name still can, which is
//! [`super::require_available`]'s existing behaviour and unchanged by
//! bl-79a2. Recorded, not widened.

use super::{GitRunner, Path, Unavailable};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The embedded pool: 541 concrete, neutral, everyday English words,
/// authored for this repository and covered by the crate's own licence
/// (bl-b59c — it replaces an EFF-derived CC BY 4.0 list, which put a
/// second licence inside an MIT package and minted names like `wrath`).
/// Provenance, the sizing argument and the invariants are in the file's
/// own header; the approval is pinned by count and digest in
/// [`tests::corpus`]. It is data, so it ships in the binary via
/// `include_str!`. The list is still sized for human review, not for
/// entropy — the pair space it spells (541 × 540 = 292,140 names) is not
/// the constraint on it, and widening the space is why bl-79a2 needed no
/// second wordlist.
const WORDS_TXT: &str = include_str!("mint/words.txt");

/// The one way a mint fails: every name the pool can spell is taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MintError {
    /// All `n` names the wordlist's pair space spells are occupied.
    #[error("name pool exhausted: all {0} names are occupied")]
    Exhausted(usize),
}

/// The injected randomness the mint is pure over. A trait rather than a
/// concrete generator so a test scripts the draw and the production
/// seeding stays out of the pure path. The draw takes `&self` — state
/// advances by interior mutability — so a generator rides the crate's
/// `&dyn` injection seams ([`crate::prompt::Deps`]) like every other
/// injected dependency.
pub trait Rng {
    /// The next 64 random bits.
    fn next_u64(&self) -> u64;
}

/// SplitMix64 — the production [`Rng`]. Chosen because it is a few lines
/// of wrapping arithmetic: the mint needs one draw per name, and a
/// `rand` dependency for that is not worth the supply-chain surface.
/// The state is atomic (the `&self` draw above); the additive stream
/// constant makes `fetch_add` the whole state advance.
#[derive(Debug)]
pub struct SplitMix64 {
    state: AtomicU64,
}

/// SplitMix64's additive stream constant.
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

impl SplitMix64 {
    /// A generator from an explicit seed — reproducible, and the seam the
    /// entropy path funnels through.
    pub const fn from_seed(seed: u64) -> Self {
        Self {
            state: AtomicU64::new(seed),
        }
    }

    /// A generator seeded from the wall clock and this process's id.
    /// Neither input is secret — the mint is a collision-avoidance
    /// device, not a security one, and the occupied-set check plus the
    /// creation-time [`super::require_available`] gate are what actually
    /// guarantee uniqueness.
    pub fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self::from_seed(nanos ^ (u64::from(std::process::id()) << 32))
    }
}

impl Rng for SplitMix64 {
    fn next_u64(&self) -> u64 {
        let z = self
            .state
            .fetch_add(GAMMA, Ordering::Relaxed)
            .wrapping_add(GAMMA);
        let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// The embedded wordlist as words: non-blank, non-comment lines, trimmed.
fn wordlist() -> Vec<&'static str> {
    WORDS_TXT
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// Two words joined with their initials upper-cased and nothing between
/// them — `("peach", "hollow")` ⇒ `PeachHollow`. Ascii-only upper-casing,
/// because the pool is pinned to `^[a-z]{3,9}$` ([`tests::corpus`]); an
/// empty entry contributes nothing rather than panicking.
fn pascal(first: &str, second: &str) -> String {
    let mut name = String::with_capacity(first.len() + second.len());
    for word in [first, second] {
        let mut chars = word.chars();
        if let Some(initial) = chars.next() {
            name.push(initial.to_ascii_uppercase());
            name.push_str(chars.as_str());
        }
    }
    name
}

/// The mint over an explicit wordlist — the whole algorithm, kept
/// list-injectable so tests exercise collision retry and exhaustion on a
/// tiny pool instead of the embedded one.
///
/// The pool is the **ordered pairs of distinct words**, `n * (n - 1)` of
/// them: index `i` picks `i / (n - 1)` as the first word and the
/// `i % (n - 1)`-th of the *others* as the second, so a word never pairs
/// with itself (`PeachPeach` is not a name the mint can spell) and every
/// pair is reachable exactly once. The retry is bounded by that pool:
/// each occupied name is discarded for the next with wraparound, and one
/// full lap proves exhaustion exactly — no free name is ever missed, no
/// loop runs unbounded. A list of fewer than two words spells no name at
/// all, which is the same empty pool the empty list is rather than a
/// case of its own. An out-of-range index cannot occur, and a missing
/// word reads as empty rather than panicking.
fn mint_from(
    words: &[&str],
    rng: &dyn Rng,
    occupied: &HashSet<String>,
) -> Result<String, MintError> {
    let others = words.len().saturating_sub(1);
    let pool = words.len() * others;
    let start = (rng.next_u64() % pool.max(1) as u64) as usize;
    for step in 0..pool {
        let index = (start + step) % pool;
        let (first, offset) = (index / others, index % others);
        let second = if offset < first { offset } else { offset + 1 };
        let name = pascal(
            words.get(first).copied().unwrap_or_default(),
            words.get(second).copied().unwrap_or_default(),
        );
        if !occupied.contains(&name) {
            return Ok(name);
        }
    }
    Err(MintError::Exhausted(pool))
}

/// Mint a name from the embedded wordlist: the first PascalCase pair of
/// distinct words not in `occupied`, scanning from an RNG-chosen start.
/// Pure — same RNG state and same occupied set, same name.
pub fn mint(rng: &dyn Rng, occupied: &HashSet<String>) -> Result<String, MintError> {
    mint_from(&wordlist(), rng, occupied)
}

/// The settle-the-name pre-flight both creation paths run before forking
/// (ARCH §2.3): a supplied name is validated against the living agents
/// ([`super::require_available`]); an absent one is minted against the
/// **same** living-names scan ([`super::named`]) — one occupied-set
/// derivation, so no fork ends nameless and no second registry exists to
/// drift. A refusal (taken, malformed, id-shaped, or an exhausted pool)
/// leaves no branch, no worktree and no inbox behind.
pub fn preflight(
    workspace: &Path,
    supplied: Option<&str>,
    git: &dyn GitRunner,
    rng: &dyn Rng,
) -> Result<String, Unavailable> {
    match supplied {
        Some(name) => {
            super::require_available(workspace, name, git)?;
            Ok(name.to_owned())
        }
        None => {
            let occupied: HashSet<String> = super::named(workspace, git)
                .map_err(Unavailable::Scan)?
                .into_iter()
                .map(|(_, name)| name)
                .collect();
            Ok(mint(rng, &occupied)?)
        }
    }
}

/// True iff `name` wears the shape the mint spells (bl-79a2): two words
/// joined in PascalCase — ASCII letters only (so no separator of any
/// kind), exactly two upper-case initials, the first of them leading.
/// The one home of that predicate, so the tests of the *other* modules
/// that assert on an omitted name — the fork's staged `name` file, a
/// child's minted name, the e2e blob — cannot each drift into their own
/// weaker reading of "minted".
#[cfg(test)]
pub(crate) fn is_minted_shape(name: &str) -> bool {
    name.chars().all(|c| c.is_ascii_alphabetic())
        && name.chars().filter(char::is_ascii_uppercase).count() == 2
        && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// A seeded, process-shared [`SplitMix64`] for tests whose subject is
/// not the mint: `'static`, so it drops into a `Deps` or a call tail
/// with no local binding. A test that asserts on the minted name
/// constructs its own seeded generator instead — this one's draw order
/// depends on what ran before it.
#[cfg(test)]
pub(crate) fn test_rng() -> &'static SplitMix64 {
    static RNG: SplitMix64 = SplitMix64::from_seed(7);
    &RNG
}

#[cfg(test)]
mod tests;
