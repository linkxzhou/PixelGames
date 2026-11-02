//! pxl-store: content addressing and persistence shared by the steps.
//!
//! Absorbed verbatim from `harness/src/{digest,storage,qa}.rs` in M2
//! (plan/v3.0/pxl-step-pipeline-plan.md §7.2): canonical JSON digests,
//! immutable revisions with manifest digests, the append-only event log, the
//! project lock, and the gate-report model.

pub mod digest;
pub mod qa;
pub mod storage;

/// Crate version recorded in run states and revision manifests (was
/// `harness::VERSION` before the M2 move).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
