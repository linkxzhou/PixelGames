//! Pipeline orchestration: stage runners drive agent tasks, run gates and
//! commit revisions. Expanded per milestone (design/assets runners land in
//! the stage modules).

pub mod design_stage;
pub mod review_view;
pub mod runner;
pub mod state;

pub use runner::{PipelineRunner, UntilStage};
