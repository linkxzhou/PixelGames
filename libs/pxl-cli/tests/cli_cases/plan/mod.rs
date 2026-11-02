//! `plan` — the batch entry between steps (plan/v3.0/pxl-step-pipeline-plan.md
//! §4.3). It must reuse the `build` pipeline byte for byte, never abort the
//! batch on one failed recipe, and write an index.json the package step can
//! consume.

mod batch;
mod discovery;
mod helpers;
