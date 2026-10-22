//! Stage state transitions shared by the runners.

use crate::error::{HarnessError, Result};
use crate::storage::{RunState, StageStatus};

/// Allowed transitions. `waiting_approval` is reachable from `validating`
/// only; gates decide, never the agent.
pub fn transition(cur: StageStatus, next: StageStatus) -> Result<()> {
    use StageStatus::*;
    let ok = matches!(
        (cur, next),
        (NotRequested, NotRequested)
            | (NotRequested, Pending)
            | (Pending, Running)
            | (Running, Validating)
            | (Validating, WaitingApproval)
            | (Validating, Running)
            | (Validating, Succeeded)
            | (WaitingApproval, Succeeded)
            | (Running, Failed)
            | (Validating, Failed)
            | (WaitingApproval, Failed)
            | (Running, Blocked)
            | (Blocked, Pending)
            | (Running, BudgetExhausted)
            | (Validating, BudgetExhausted)
            | (Succeeded, Stale)
            | (Failed, Pending)
            | (Cancelled, Pending)
            | (Pending, Cancelled)
            | (Running, Cancelled)
        );
    if ok {
        Ok(())
    } else {
        Err(HarnessError::semantic(format!(
            "illegal stage transition {} -> {}",
            cur.as_str(),
            next.as_str()
        ))
        .with_hint("state machine: pending→running→validating→waiting_approval→succeeded"))
    }
}

pub fn set_stage(state: &mut RunState, stage: &str, next: StageStatus) -> Result<()> {
    let cur = state
        .stages
        .get(stage)
        .copied()
        .ok_or_else(|| HarnessError::not_found(format!("unknown stage '{stage}'")))?;
    transition(cur, next)?;
    state.stages.insert(stage.to_string(), next);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_are_restricted() {
        assert!(transition(StageStatus::Pending, StageStatus::Running).is_ok());
        assert!(transition(
            StageStatus::Running,
            StageStatus::WaitingApproval
        )
        .is_err());
        assert!(transition(
            StageStatus::Validating,
            StageStatus::WaitingApproval
        )
        .is_ok());
        assert!(transition(StageStatus::Validating, StageStatus::Running).is_ok());
        assert!(transition(StageStatus::Validating, StageStatus::Succeeded).is_ok());
        assert!(transition(StageStatus::Succeeded, StageStatus::Running).is_err());
        assert!(transition(StageStatus::Succeeded, StageStatus::Stale).is_ok());
    }

    #[test]
    fn set_stage_updates_run_state() {
        let mut state = RunState::new("r".into(), "d".into(), "c".into(), "design");
        set_stage(&mut state, "design", StageStatus::Running).unwrap();
        assert_eq!(state.stages["design"], StageStatus::Running);
        assert!(set_stage(&mut state, "design", StageStatus::Succeeded).is_err());
        assert!(set_stage(&mut state, "nope", StageStatus::Running).is_err());
    }
}
