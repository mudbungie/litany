//! The mint, ported semantics-intact from yog `src/names` (bl-aca4):
//! RNG-start wraparound draw, bounded collision retry, loud exhaustion —
//! plus the settle-the-name pre-flight over a real workspace. Since
//! bl-79a2 the drawn name is a **PascalCase pair of distinct words**, so
//! the scan's index space is the pair space and the shape assertions
//! below are what pin the two-word join.

use super::*;
use crate::template::{GitRunner, RealGit};
use crate::workspace::fixture;

/// An [`Rng`] that yields one scripted draw forever — the mint takes
/// exactly one, so a fixed value pins the scan's start index.
struct Fixed(u64);

impl Rng for Fixed {
    fn next_u64(&self) -> u64 {
        self.0
    }
}

fn set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

/// Three words ⇒ a **six**-name pool (3 × 2 ordered distinct pairs),
/// indices 0..6 in scan order: `AshBay AshCove BayAsh BayCove CoveAsh
/// CoveBay`.
const ABC: &[&str] = &["ash", "bay", "cove"];

/// That scan order, written out — the one place the index mapping is
/// stated as data rather than derived, so a mapping change has to face
/// it.
const ABC_PAIRS: &[&str] = &[
    "AshBay", "AshCove", "BayAsh", "BayCove", "CoveAsh", "CoveBay",
];

/// A wordlist, the RNG draw, the occupied names, and the expected mint.
type Case = (
    &'static [&'static str],
    u64,
    &'static [&'static str],
    Result<&'static str, MintError>,
);

#[test]
fn mint_scans_from_the_draw_and_retries_past_collisions() {
    let cases: &[Case] = &[
        // Draw picks the start index into the PAIR space; nothing occupied
        // ⇒ that pair is the name.
        (ABC, 0, &[], Ok("AshBay")),
        (ABC, 2, &[], Ok("BayAsh")),
        (ABC, 5, &[], Ok("CoveBay")),
        // A draw wider than the pool wraps into it (9 % 6 == 3).
        (ABC, 9, &[], Ok("BayCove")),
        // Collision retry: the start is taken, discarded, the scan re-samples
        // the next pair — which is the next SECOND word, not a fresh draw.
        (ABC, 0, &["AshBay"], Ok("AshCove")),
        // Retry wraps past the end of the pool: "CoveBay" then "AshBay" taken.
        (ABC, 5, &["CoveBay", "AshBay"], Ok("AshCove")),
        // Pool exhaustion: a two-word list spells exactly two names — the
        // retry is bounded by the pool, so this errors instead of looping.
        (
            &["ash", "bay"],
            0,
            &["AshBay", "BayAsh"],
            Err(MintError::Exhausted(2)),
        ),
        // One word spells NO name: a word never pairs with itself, so this
        // is the empty pool rather than a case of its own.
        (&["ash"], 0, &[], Err(MintError::Exhausted(0))),
        // The empty list is the general path with no inputs — an empty pool.
        (&[], 0, &[], Err(MintError::Exhausted(0))),
        // An empty entry contributes nothing to the join rather than
        // panicking — the join is total over whatever the list holds.
        (&["", "bay"], 0, &[], Ok("Bay")),
    ];
    for (words, draw, taken, expect) in cases {
        let got = mint_from(words, &Fixed(*draw), &set(taken));
        let want = expect.clone().map(str::to_owned);
        assert_eq!(got, want, "words={words:?} draw={draw} taken={taken:?}");
    }
}

/// The pair space is enumerated exactly once per name and stops dead at
/// its own bound: six mints off a three-word list yield the six ordered
/// distinct pairs in scan order, a seventh is [`MintError::Exhausted`],
/// and no self-pair is ever spelled.
#[test]
fn the_scan_spells_every_distinct_pair_once_then_exhausts() {
    let mut occupied = HashSet::new();
    let mut minted = Vec::new();
    for _ in 0..ABC_PAIRS.len() {
        let name = mint_from(ABC, &Fixed(0), &occupied).expect("a free pair remains");
        occupied.insert(name.clone());
        minted.push(name);
    }
    assert_eq!(minted, ABC_PAIRS, "the scan order is the index mapping");
    assert_eq!(
        mint_from(ABC, &Fixed(0), &occupied),
        Err(MintError::Exhausted(6)),
        "the pair space is the bound, and it is exact"
    );
    for word in ABC {
        let self_pair = pascal(word, word);
        assert!(!occupied.contains(&self_pair), "{self_pair:?} was spelled");
    }
}

#[test]
fn exhaustion_is_loud_at_both_altitudes() {
    let err = MintError::Exhausted(2);
    assert_eq!(
        err.to_string(),
        "name pool exhausted: all 2 names are occupied"
    );
    // The pre-flight's projection is transparent: the caller's uniform
    // failure line carries the same words.
    assert_eq!(Unavailable::from(err.clone()).to_string(), err.to_string());
}

/// The embedded pool's own invariants — shape, approval, and semantic
/// safety — in their own file, because they are about the *data* and not
/// about the algorithm the rest of this file exercises.
mod corpus;

/// Split a minted name back into the two pool words it joined, proving
/// the shape on the way: exactly two ASCII-uppercase initials, no
/// separator of any kind, and both halves entries of the embedded pool.
fn split_pascal(name: &str) -> (String, String) {
    assert!(
        is_minted_shape(name),
        "{name:?} is not two PascalCase words with no separator"
    );
    let cut = name
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_ascii_uppercase())
        .map(|(i, _)| i)
        .expect("the shape check found two capitals");
    let (first, second) = name.split_at(cut);
    let (first, second) = (first.to_lowercase(), second.to_lowercase());
    let pool = wordlist();
    assert!(
        pool.contains(&first.as_str()),
        "{first:?} is not in the pool"
    );
    assert!(
        pool.contains(&second.as_str()),
        "{second:?} is not in the pool"
    );
    assert_ne!(first, second, "{name:?} pairs a word with itself");
    (first, second)
}

/// The shared shape predicate reads the ruling and nothing looser: one
/// word fails it, a hyphenated pair fails it, three words fail it, and a
/// lower-cased initial fails it — otherwise the other modules' minted-name
/// assertions would pass on the very shapes bl-79a2 replaced.
#[test]
fn the_minted_shape_predicate_reads_only_two_pascal_words() {
    assert!(is_minted_shape("PeachHollow"));
    for wrong in [
        "",
        "peach",
        "Peach",
        "peachHollow",
        "PeachHollowGate",
        "Peach-Hollow",
        "Peach Hollow",
        "peach-hollow",
    ] {
        assert!(!is_minted_shape(wrong), "{wrong:?} read as minted");
    }
}

#[test]
fn mint_over_the_embedded_list_is_deterministic_and_avoids_the_occupied_set() {
    // Deterministic per seed: the same generator state mints the same name.
    let name = mint(&SplitMix64::from_seed(7), &HashSet::new()).unwrap();
    let again = mint(&SplitMix64::from_seed(7), &HashSet::new()).unwrap();
    assert_eq!(name, again);
    // The two-word PascalCase shape, over the real 541-word pool.
    let (first, second) = split_pascal(&name);
    assert_eq!(name, pascal(&first, &second));
    // Occupying that name pushes the mint to a different one, still shaped.
    let next = mint(&SplitMix64::from_seed(7), &set(&[&name])).unwrap();
    assert_ne!(next, name);
    split_pascal(&next);
}

#[test]
fn splitmix64_advances_and_seeds_from_entropy() {
    let rng = SplitMix64::from_seed(0);
    let draws: HashSet<u64> = (0..64).map(|_| rng.next_u64()).collect();
    assert_eq!(draws.len(), 64, "generator repeated within 64 draws");
    // The entropy path is real seeding, not a constant.
    assert_ne!(
        SplitMix64::from_entropy().next_u64(),
        SplitMix64::from_seed(0).next_u64()
    );
}

#[test]
fn preflight_validates_a_supplied_name_and_mints_on_omission() {
    let (_h, ws) = fixture::workspace();
    let git = RealGit::new();
    // A living agent wearing a name occupies it for both arms.
    let wt = fixture::spawn_root(&ws, "20260101T000000Z-aaaaaaaa");
    super::super::settle(&wt, Some("pale-otter"), &git).unwrap();
    git.run(&wt, &["commit", "-m", "settle name"]).unwrap();

    // Supplied → the ordinary uniqueness gate, unchanged.
    assert_eq!(
        preflight(&ws, Some("brook"), &git, &Fixed(0)).unwrap(),
        "brook"
    );
    assert!(matches!(
        preflight(&ws, Some("pale-otter"), &git, &Fixed(0)),
        Err(Unavailable::Taken { .. })
    ));
    // Absent → minted against the same living-names scan, deterministic
    // under the injected RNG, and never a name a living agent wears.
    let minted = preflight(&ws, None, &git, &SplitMix64::from_seed(7)).unwrap();
    assert_eq!(
        minted,
        preflight(&ws, None, &git, &SplitMix64::from_seed(7)).unwrap()
    );
    split_pascal(&minted);
    assert_ne!(minted, "pale-otter");
    // An unreadable workspace is the scan's refusal, not a raw git error.
    assert!(matches!(
        preflight(Path::new("/nonexistent"), None, &git, &Fixed(0)),
        Err(Unavailable::Scan(_))
    ));
}

/// **The hazard the purity contract exists for** (bl-8fe8): two
/// generators seeded from the same *wall-clock second* mint the same
/// name, and the occupied-set check cannot tell them apart because
/// concurrent creators scan before any of them has committed a `name`.
///
/// Written as the failure rather than as the property, because the
/// property — "same seed, same name", pinned above — reads as a
/// guarantee, and a consumer reading it as one shipped exactly this: yog
/// seeded from a hash of its unix-**seconds** ops-log stamp, and three
/// conversations started in one second were all minted one name, which
/// every seat verb then refused as ambiguous. The occupied set is empty
/// in both draws here for the same reason it is empty in the wild: none
/// of the racing creators has landed a dispatch commit yet.
#[test]
fn two_generators_seeded_from_one_wall_clock_second_mint_one_name() {
    // Whatever a caller derives a seed from, two creations inside one
    // grain of it hand the mint the same number.
    let seed_of_this_second = 0x5eed_0000_0000_0001;
    let first = mint(&SplitMix64::from_seed(seed_of_this_second), &HashSet::new()).unwrap();
    let second = mint(&SplitMix64::from_seed(seed_of_this_second), &HashSet::new()).unwrap();
    assert_eq!(
        first, second,
        "a seed shared by two creations mints one name — the consumer owes per-creation entropy"
    );
    // And the finer grain is what dissolves it: the engine's own path.
    assert_ne!(
        mint(&SplitMix64::from_entropy(), &HashSet::new()).unwrap_or_default(),
        String::new(),
        "the entropy path mints"
    );
}
