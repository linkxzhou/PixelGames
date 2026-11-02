//! Goal subsystem: a harness-owned acceptance loop on top of the agent
//! runner. Structural port of miniagent `goal/`, adapted to batch mode.

pub mod evaluator;
pub mod models;
pub mod runtime;

pub use evaluator::GoalEvaluator;
pub use models::{load_goal, save_goal, EvalResult, GoalRecord, GoalStatus};
pub use runtime::GoalRuntime;
