//! Persistence layer: project identity, run state, append-only events,
//! immutable revisions, budgets.

pub mod artifact_store;
pub mod event_log;
pub mod run_store;

pub use artifact_store::{ArtifactStore, ProjectIdentity, BRIEF_FILE, PROJECT_FILE};
pub use event_log::{Event, EventLog, LoggedEvent, EVENTS_FILE};
pub use run_store::{ProjectLock, RunState, RunStore, StageStatus, TaskState, RUN_FILE, STAGES};
