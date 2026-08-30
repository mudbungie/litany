//! Inference-provider runtime types.
//!
//! Per `docs/ARCHITECTURE.md` §4.1, a provider is an (endpoint, auth)
//! pair — a **brazen** provider-row name (§4.4). The harness speaks no
//! provider wire protocol directly: every model call crosses the `bz`
//! subprocess boundary (§3.4). The canonical request/event *types* are
//! the linked `brazen` crate's (`CanonicalRequest`, `Content`, `Event`,
//! `CanonicalError`); litany carries no bespoke wire types.
//!
//! [`segment`] classifies a closed `response.json`'s last attempt
//! segment (§4.4) over brazen's `v=1` event vocabulary — the single
//! framing seam every reader shares.

pub mod segment;
