//! The paired verdict on a pass@1 delta (ARCH §9.3, bl-a35e): the exact
//! two-sided sign test over per-task pass rates, its counts, and the
//! two shapes that carry no verdict at all.

use agent_eval::paired::{self, ALPHA};

/// `(baseline, candidate)` pairs from two rate lists of equal length.
fn pairs(b: &[f64], c: &[f64]) -> Vec<(f64, f64)> {
    b.iter().copied().zip(c.iter().copied()).collect()
}

#[test]
fn the_counts_split_the_shared_tasks_into_better_worse_and_tied() {
    let t = paired::sign_test(&pairs(&[0.0, 0.4, 0.8, 1.0], &[0.2, 0.4, 0.2, 1.0]));
    assert_eq!((t.better, t.worse, t.tied), (1, 1, 2));
    assert_eq!(t.moved(), 2);
}

#[test]
fn a_unanimous_move_reaches_significance_at_the_conventional_threshold() {
    // Six tasks improved, none worsened: the two-sided exact p is
    // 2 * (1/2)^6 = 0.03125, which clears 0.05 — and the smallest
    // unanimous sample that does. Five would be 0.0625.
    let t = paired::sign_test(&pairs(&[0.0; 6], &[1.0; 6]));
    assert_eq!((t.better, t.worse, t.tied), (6, 0, 0));
    assert!((t.p_value.unwrap() - 0.031_25).abs() < 1e-12);
    assert!(t.significant());

    let five = paired::sign_test(&pairs(&[0.0; 5], &[1.0; 5]));
    assert!((five.p_value.unwrap() - 0.0625).abs() < 1e-12);
    assert!(!five.significant(), "0.0625 does not clear {ALPHA}");
}

#[test]
fn an_even_split_is_the_null_and_its_two_sided_p_is_capped_at_one() {
    // Doubling one tail overshoots at or below the median; the cap is
    // what keeps a p-value a probability.
    let t = paired::sign_test(&pairs(&[0.0, 0.0, 1.0, 1.0], &[1.0, 1.0, 0.0, 0.0]));
    assert_eq!((t.better, t.worse), (2, 2));
    assert_eq!(t.p_value, Some(1.0));
    assert!(!t.significant());
}

#[test]
fn nothing_moving_is_the_absence_of_a_verdict_not_a_p_of_one() {
    // Every task tied, and an empty comparison: both have an empty null
    // distribution, so there is nothing to test. A reported 1.0 would
    // read as a measured answer rather than as no answer.
    let tied = paired::sign_test(&pairs(&[0.4, 1.0], &[0.4, 1.0]));
    assert_eq!((tied.better, tied.worse, tied.tied), (0, 0, 2));
    assert_eq!(tied.p_value, None);
    assert!(!tied.significant());

    let empty = paired::sign_test(&[]);
    assert_eq!(empty.p_value, None);
    assert_eq!(empty.moved(), 0);
}

#[test]
fn the_direction_is_read_and_the_size_is_not() {
    // The test's stated price: one task moving hugely the right way and
    // three moving slightly the wrong way is a majority against, and the
    // sign test says so. The per-task block is where the sizes live.
    let t = paired::sign_test(&pairs(&[0.0, 1.0, 1.0, 1.0], &[1.0, 0.8, 0.8, 0.8]));
    assert_eq!((t.better, t.worse), (1, 3));
    assert!(!t.significant());
}

#[test]
fn a_large_task_count_stays_a_probability() {
    // The pmf recurrence rather than factorials: `C(200, 100)` has no
    // integer type, while every partial sum here stays in [0, 1].
    let t = paired::sign_test(&pairs(&[0.0; 200], &[1.0; 200]));
    let p = t.p_value.unwrap();
    assert!(p > 0.0 && p < 1e-50, "a real, tiny probability: {p}");
}
