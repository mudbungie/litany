//! The operator-notice voice on stderr (ARCH §2.11 *stderr is different
//! in kind*).
//!
//! A driver's stderr carries two populations that look identical to a
//! reader: **operator notices** — the Ok-path lines stating what the
//! harness declined or accepted and stepped past (a compaction landing
//! declined or superseded, §2.6; a budget stop, §6; a launch that fell
//! into the accepted crash class, §2.11; a retarget decline, §2.2) — and
//! whatever a dying process writes on its way out. The first population
//! is diagnostic and the process continues; the second is a death
//! rattle. Nothing about the stream separates them, and a `setsid`
//! driver's stderr is captured to `steps/<agent-id>/driver.log`
//! (§2.11), so the party reading it is a *program*, not the operator
//! standing at a terminal.
//!
//! [`PREFIX`] is that separator, and it is the whole mechanism: an
//! operator notice is exactly a line beginning `litany: notice: `. A
//! consumer keys on the prefix instead of phrase-matching prose, which
//! is what it had to do before and which broke on every rewording. The
//! contract is the prefix and the fact that a prefixed line does not
//! imply failure — never the sentence after it, which stays free to be
//! reworded, and never the exit code, which the notice does not touch.
//!
//! **What is not a notice.** A verb's own confirmation to the operator
//! who just typed it (`litany retarget`'s mark, `litany prime`'s
//! founding report, `litany message`'s failed-branch advisory) keeps the
//! bare `litany: ` voice: it is spoken to somebody present, it never
//! reaches a driver's captured sink, and marking it would say "not a
//! failure" to a reader who never suspected one. Neither is a fatal
//! error — a process about to exit non-zero is the population the prefix
//! exists to be distinguished *from*.

/// The prefix every operator notice carries, trailing separator
/// included, so [`line`] is a concatenation and nothing composes the
/// spacing a second time. This constant is the contract a consumer keys
/// on; it has one home and the [`notice`] macro is its only caller.
pub(crate) const PREFIX: &str = "litany: notice: ";

/// Compose one operator-notice line. Split out from [`notice`] so the
/// composition is a value a test can assert on: the macro's own effect
/// is a write to the process's real stderr, which a unit test cannot
/// address.
pub(crate) fn line(body: std::fmt::Arguments) -> String {
    format!("{PREFIX}{body}")
}

/// Emit one operator notice on stderr — `eprintln!`'s formatting, with
/// [`PREFIX`] in front. Every site that speaks the notice voice uses
/// this, so a site cannot spell the prefix wrong or forget it.
macro_rules! notice {
    ($($arg:tt)*) => {
        eprintln!("{}", $crate::prompt::notice::line(format_args!($($arg)*)))
    };
}

pub(crate) use notice;

#[cfg(test)]
mod tests;
