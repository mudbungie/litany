//! Every way driving an agent can fail (ARCH §2, §4.4, §6).
//!
//! One taxonomy for the whole executor — the step loop, the config
//! reads, the adapter, the dispatch gate, the inbox — deliberately
//! narrower than brazen's: wire-level distinctions are brazen's, spoken
//! in band as the `CanonicalError` this enum folds into
//! [`Error::AdapterError`] (§4.4). It lives beside [`crate::prompt::run`]
//! rather than inside it because it is the module's shared vocabulary,
//! not one function's.
//!
//! **The taxonomy is one enum and stays one enum**, so the split here is
//! between the vocabulary and its *derivations*, never within it:
//! [`kinds`] declares [`Error`] and nothing else, [`from_adapter`] folds
//! brazen's in-band `CanonicalError` into it (§4.4), and this root is the
//! module's own subject — what belongs in the taxonomy and what does
//! not. Split at the per-file cap when the §4.3 role decline joined it
//! (bl-946c); nesting a sub-enum was the alternative and was refused,
//! because a flat taxonomy is what the paragraph above asserts.

mod from_adapter;
mod kinds;

pub use kinds::Error;
