//! **A verdict on the delta, paired per task** (ARCH §9.3, §12; bl-a35e).
//!
//! `compare` renders each side's pass@1 with its own Wilson interval and
//! the difference between them, which answers *how big* and never
//! *whether*. The v1.0 criterion (§12) asks for a variant that beats
//! baseline "by a statistically significant margin on pass@1", so the
//! report has to state the significance rather than leave a reader to
//! eyeball two overlapping intervals — a comparison whose intervals
//! overlap can still be significant, and one whose intervals do not can
//! still be a handful of tasks moving.
//!
//! **Paired per task, because the runs are not independent.** An
//! evaluation is N runs of each task, and the runs within one task share
//! everything the task is: its prompt, its fixture, its checker. Pooling
//! every run of every task into one two-proportion test would treat 50
//! tasks × 5 runs as 250 independent Bernoulli trials, which they are
//! not — it understates the standard error and manufactures significance
//! out of clustering. The honest unit is the **task**: each contributes
//! one paired observation, its pass rate under baseline against its pass
//! rate under the candidate, and the two arms saw the same task.
//!
//! **The sign test, and why it is the one named here.** It is *exact* —
//! the null distribution is a binomial with p = 1/2, computed in closed
//! form — so it needs no normal approximation, no variance estimate, and
//! no dependency (`docs/PRINCIPLES.md`: the crate adds none for this).
//! Its assumptions are the two the design already satisfies: the pairs
//! are independent of each other (distinct tasks) and, under the null,
//! a task is as likely to improve as to worsen. McNemar's test is the
//! same statistic on a 2×2 table of *binary* per-task outcomes; the sign
//! test is stated over the per-task **rates**, which is what an
//! evaluation with N runs per task actually measures, and reduces to
//! McNemar's exact form when N = 1.
//!
//! Its price, stated rather than hidden: the sign test reads only the
//! *direction* of each task's change, never its size, so a variant that
//! moves a few tasks enormously and the rest slightly the wrong way will
//! not reach significance. That is a conservative failure, and the
//! per-task block above it in the report is where the sizes are.
//!
//! **Ties are discarded, not counted.** A task whose pass rate did not
//! move carries no evidence about direction; including it as half a
//! success is a different (and approximate) test. The count is reported,
//! so a verdict resting on three moving tasks out of fifty says so.

/// The paired verdict over one comparison's shared tasks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignTest {
    /// Tasks whose pass rate rose under the candidate.
    pub better: usize,
    /// Tasks whose pass rate fell.
    pub worse: usize,
    /// Tasks whose pass rate did not move — no evidence either way.
    pub tied: usize,
    /// Two-sided exact p-value, or `None` when no task moved at all:
    /// with an empty null distribution there is nothing to test, and a
    /// reported `1.0` would read as a measured answer rather than the
    /// absence of one (the same rule the report gives a missing metric).
    pub p_value: Option<f64>,
}

/// The conventional two-sided threshold this report calls significant.
/// One home, so the sentence and the test cannot disagree.
pub const ALPHA: f64 = 0.05;

impl SignTest {
    /// Tasks that moved — the sign test's actual sample size, which is
    /// what a reader must weigh the p-value against.
    pub fn moved(&self) -> usize {
        self.better + self.worse
    }

    /// Does the evidence clear [`ALPHA`]? `None` (nothing moved) is not
    /// significant — absence of evidence, stated as such.
    pub fn significant(&self) -> bool {
        self.p_value.is_some_and(|p| p < ALPHA)
    }
}

/// Run the test over `pairs` — one `(baseline_rate, candidate_rate)` per
/// shared task, in any order.
pub fn sign_test(pairs: &[(f64, f64)]) -> SignTest {
    let mut better = 0usize;
    let mut worse = 0usize;
    for (b, c) in pairs {
        if c > b {
            better += 1;
        } else if c < b {
            worse += 1;
        }
    }
    let n = better + worse;
    SignTest {
        better,
        worse,
        tied: pairs.len() - n,
        p_value: (n > 0).then(|| two_sided(n, better.max(worse))),
    }
}

/// Two-sided exact binomial p at p = 1/2: twice the upper tail from `k`,
/// capped at 1 (the cap bites when `k` is at or below the median, where
/// doubling one tail overshoots).
fn two_sided(n: usize, k: usize) -> f64 {
    (2.0 * upper_tail(n, k)).min(1.0)
}

/// `P(X >= k)` for `X ~ Binomial(n, 1/2)`, summed from the pmf's own
/// recurrence rather than from factorials — `C(n, j)` overflows every
/// integer type long before an evaluation's task count does, while the
/// pmf never leaves `[0, 1]`.
fn upper_tail(n: usize, k: usize) -> f64 {
    let mut pmf = 0.5f64.powi(i32::try_from(n).unwrap_or(i32::MAX));
    let mut tail = 0.0;
    for j in 0..=n {
        if j >= k {
            tail += pmf;
        }
        // pmf(j+1) = pmf(j) * (n - j) / (j + 1)
        pmf = pmf * (n - j) as f64 / (j + 1) as f64;
    }
    tail
}
