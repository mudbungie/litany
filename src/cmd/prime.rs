//! `litany prime` — found the installation substrate idempotently (ARCH
//! §2.2): resolve the harness root and seed the default `models.yaml`,
//! the tool/skill pools, and the `workflows/`/`workspaces/` dirs,
//! seed-if-absent. `make install` runs it.
//!
//! **It says what it founded, on stderr (bl-7e9e).** `prime` is the first
//! command a fresh install runs, and its whole effect is on disk in two
//! XDG roots the user did not choose and cannot see from the invocation
//! — so a silent success left a new user unable to tell a founded root
//! from a no-op. The report names both roots, what lives in each, and the
//! seed-if-absent split for this run (files written vs files already
//! there and left alone), which is also the re-run answer: `0 files
//! seeded` *is* "already founded". It is a **confirmation, not a
//! product**, so it goes to stderr and the verb stays
//! [`Outcome::Quiet`](super::Outcome::Quiet) — stdout carries one product
//! per verb and `prime` has none (§3.4). Founding done on the way to
//! another verb (`litany new`, §2.2) reports nothing: that verb's product
//! is its own, and its founding is a precondition, not the act asked for.

use super::{Error, Fx, Outcome};
use crate::harness_root::{self, Roots};
use crate::install::Founding;

/// `litany prime` — takes no arguments.
#[derive(clap::Args, Debug)]
pub struct Args {}

/// Seed the harness root, then report it on stderr — product-less on
/// stdout (§3.4). Failures — root resolution or seeding — carry the
/// `prime` prefix through one conversion.
pub fn run(_args: Args, _fx: &mut Fx) -> Result<Outcome, Error> {
    go().map_err(|e| Error::new("prime", e))
}

fn go() -> Result<Outcome, Box<dyn std::error::Error>> {
    let roots = harness_root::resolve()?;
    let founding = crate::install::prime(&roots)?;
    eprintln!("{}", report(&roots, &founding));
    Ok(Outcome::Quiet)
}

/// The stderr report: the two roots with what each holds, then this
/// run's seed-if-absent split (§2.2). Pure, so the wording is tested
/// without a process.
fn report(roots: &Roots, founding: &Founding) -> String {
    let Founding { seeded, kept } = *founding;
    format!(
        "litany prime: config root {} — models.yaml, workflows/\n\
         litany prime: data root {} — tools/, skills/, workspaces/\n\
         litany prime: harness root founded: {seeded} files seeded, {kept} already present \
         and left alone (seed-if-absent, ARCH §2.2)",
        roots.config.display(),
        roots.data.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn roots() -> Roots {
        Roots {
            config: PathBuf::from("/xc/litany"),
            data: PathBuf::from("/xd/litany"),
        }
    }

    /// A first run names both roots, what each holds, and the count it
    /// wrote — the "did it work, and where did my state go" answer.
    #[test]
    fn report_names_both_roots_and_what_this_run_seeded() {
        let r = report(
            &roots(),
            &Founding {
                seeded: 15,
                kept: 0,
            },
        );
        assert_eq!(
            r,
            "litany prime: config root /xc/litany — models.yaml, workflows/\n\
             litany prime: data root /xd/litany — tools/, skills/, workspaces/\n\
             litany prime: harness root founded: 15 files seeded, 0 already present \
             and left alone (seed-if-absent, ARCH §2.2)"
        );
    }

    /// The re-run answer is the same sentence with the counts swapped —
    /// "already founded" stated in numbers, not a second code path.
    #[test]
    fn report_on_a_re_run_says_nothing_was_re_seeded() {
        let r = report(
            &roots(),
            &Founding {
                seeded: 0,
                kept: 15,
            },
        );
        assert!(
            r.ends_with(
                "0 files seeded, 15 already present and left alone (seed-if-absent, ARCH §2.2)"
            ),
            "{r}"
        );
    }
}
