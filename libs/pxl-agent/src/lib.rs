//! pxl-agent: the model-provider loop shared by step-100-design and
//! step-200-art-spec.
//!
//! Absorbed verbatim from `harness/src/{agent,policy,config,skills,adapters/
//! process}.rs` in M2 (plan/v3.0/pxl-step-pipeline-plan.md §7.2). Nothing here
//! is a pipeline stage, so this lives in `libs/` and never carries a step
//! number.
//!
//! Layering: `pxl-contract` (errors) ← `pxl-store` (digest/events) ← this.

pub mod agent;
pub mod config;
pub mod policy;
pub mod process;
pub mod pxl_adapter;
pub mod skills;

/// Crate version reported in MCP clientInfo (was `harness::VERSION`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
