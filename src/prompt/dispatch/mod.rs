//! Branch orchestration (ARCH §2.3–§2.10): the step machinery both
//! drivers share, and the two drivers themselves.
//!
//! One step loop, two entries. [`exchange::run_exchange`] is the
//! in-process root driver behind `litany prompt`; [`advance`] is the §6
//! hop verb every launch seam spawns, and its [`advance::hop`] step body
//! mirrors the exchange loop's (§6 *one struct, two drivers*). Both read
//! the same [`Resolved`] shape ([`resolved`]), resolve it **at each step
//! boundary** (§2.2 follow-the-tip, §6 the workflow mark), and finish
//! through the same §2.11 terminal tail ([`terminal::conclude`]).
//!
//! Everything below them is a step's own machinery, split by act: the
//! inbox [`drain`] (§2.11), delivered-child-result interpretation
//! ([`child_result`], §6), context [`assembler`]y (§5), the request's
//! [`tools`] array (§3.3), the retry-driven [`model_call`] (§4.4), the
//! [`transcript`] writer (§2.3), the [`tool_step`] window (§2.5), the
//! result [`result_deposit`] (§2.6) and the [`terminal`] tail (§2.11).

pub mod advance;
mod assembler;
mod canonical;
mod child_result;
mod drain;
pub mod driver;
mod entry;
mod exchange;
mod model_call;
mod resolved;
mod result_deposit;
mod staging;
pub(crate) mod step_commit;
pub mod stop_signal;
mod terminal;
mod tool_step;
mod tools;
mod transcript;
mod transfer;

pub(super) use exchange::run_exchange;
pub use model_call::{RealSleeper, Sleeper};
pub(super) use resolved::Resolved;
pub(crate) use step_commit::inherited::prune_inherited_dialog;
pub(crate) use step_commit::{Grant, Undescribed, require_described, trim_to_context};
pub use stop_signal::{flag as stop_flag, install as install_stop_handler};
pub(crate) use transcript::MESSAGES_DIR;
