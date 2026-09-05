//! `agent-eval` — the experiment × suite × N evaluation runner (ARCH
//! §9.3, v0.10).
//!
//! An **experiment** is a `workflow.yaml` variant under `experiments/`
//! ([`experiment`]); the **suite** is the task set under `tests/suite/`
//! ([`suite`]). The [`runner`] executes experiment × suite × N — seeding
//! an isolated workspace per run, running the task `setup`, invoking the
//! agent through the [`agent`] seam, then running the task `check` (exit
//! 0 the sole pass signal) — recording per run the outer wall time and,
//! when the driver disclosed a workspace, the derived efficiency
//! [`metrics`] (bl-36fa). [`stats`] aggregates pass/fail into pass@1
//! (with 95% Wilson intervals) and pass@5, overall and per category;
//! [`report`] renders one evaluation; a saved [`record`] (provenance
//! from [`repro`] plus the observations) is what [`compare`] consumes
//! for baseline → candidate deltas.
//!
//! The agent invocation is behind a trait ([`agent::Agent`]) so the whole
//! runner is testable without live model traffic; the production
//! implementation shells out to an external harness-driver binary.

pub mod agent;
pub mod compare;
pub mod experiment;
pub mod metrics;
pub mod paired;
pub mod record;
pub mod report;
pub mod repro;
pub mod runner;
pub mod stats;
pub mod suite;
