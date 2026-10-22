//! Pipeline runner: decides which stages run, enforces dependencies and
//! approval gates, persists run state after every transition and maps the
//! final stage status onto the frozen exit-code table.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::provider::{FakeProvider, ModelProvider, OpenAiProvider};
use crate::config::{Config, ProviderKind};
use crate::error::{exit, HarnessError, Result};
use crate::policy::budget::BudgetLedger;
use crate::storage::{
    ArtifactStore, Event, EventLog, ProjectLock, RunState, RunStore, StageStatus, STAGES,
};

/// The requested pipeline end point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UntilStage {
    Design,
    Assets,
}

impl UntilStage {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "design" => Ok(Self::Design),
            "assets" => Ok(Self::Assets),
            other => Err(HarnessError::usage(format!(
                "unknown --until stage '{other}'"
            ))
            .with_hint("valid stages: design, assets")),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            UntilStage::Design => "design",
            UntilStage::Assets => "assets",
        }
    }
}

/// Everything `run` needs, resolved once up front.
pub struct PipelineRunner {
    pub project: PathBuf,
    pub until: UntilStage,
    pub cfg: Config,
    artifacts: ArtifactStore,
    run: RunStore,
    state: RunState,
    ledger: BudgetLedger,
    events: EventLog,
    _lock: ProjectLock,
}

impl PipelineRunner {
    /// Acquire the project lock, create (or open) the run directory and
    /// persist the run state before any model call.
    pub fn start(project: PathBuf, until: UntilStage) -> Result<Self> {
        let lock = ProjectLock::acquire(&project)?;
        let cfg = Config::load(&project)?;
        let artifacts = ArtifactStore::open(project.clone())?;
        // The brief snapshot must already exist; a run never rewrites inputs.
        if !artifacts.brief_path().is_file() {
            return Err(HarnessError::not_found(
                "brief.json snapshot is missing; run `pxl-harness init` first",
            ));
        }
        let brief_text = std::fs::read_to_string(artifacts.brief_path())?;
        let brief_digest = crate::digest::digest_bytes(brief_text.as_bytes());
        let config_digest = crate::digest::digest_bytes(
            serde_json::to_string(&cfg)?.as_bytes(),
        );
        let run_id = format!("r{}", timestamp_id());
        let state = RunState::new(
            run_id.clone(),
            brief_digest,
            config_digest,
            until.as_str(),
        );
        let run_dir = artifacts.runs_dir().join(&run_id);
        let run = RunStore::create(run_dir, &state)?;
        let mut events = EventLog::open(run.dir())?;
        events.append(Event::RunCreated {
            harness_version: crate::VERSION.to_string(),
            config_digest: state.config_digest.clone(),
        })?;
        let ledger = BudgetLedger::from_config(&cfg);
        ledger.save(run.dir())?;
        Ok(Self {
            project,
            until,
            cfg,
            artifacts,
            run,
            state,
            ledger,
            events,
            _lock: lock,
        })
    }

    /// Execute pending stages in order until the requested end point.
    pub fn run(&mut self, provider: &mut dyn ModelProvider) -> Result<RunSummary> {
        let result = self.run_stages(provider);
        self.state.status = match &result {
            Ok(summary) => summary.status.clone(),
            Err(error) => match error.code {
                crate::error::ErrorCode::Budget => "budget_exhausted".into(),
                crate::error::ErrorCode::Blocked | crate::error::ErrorCode::Policy => "blocked".into(),
                _ => "failed".into(),
            },
        };
        if result.is_err() {
            let status = match self.state.status.as_str() {
                "budget_exhausted" => StageStatus::BudgetExhausted,
                "blocked" => StageStatus::Blocked,
                _ => StageStatus::Failed,
            };
            for stage in self.state.stages.values_mut() {
                if matches!(*stage, StageStatus::Running | StageStatus::Validating) {
                    *stage = status;
                }
            }
        }
        self.ledger.save(self.run.dir())?;
        self.run.save(&self.state)?;
        self.events.append(Event::RunStatus { status: self.state.status.clone() })?;
        result
    }

    fn run_stages(&mut self, provider: &mut dyn ModelProvider) -> Result<RunSummary> {
        let stages = stages_up_to(self.until);
        for stage in &stages {
            if self.state.stages[stage] == StageStatus::Succeeded {
                continue;
            }
            // When an approved design exists, `run --until assets` reuses it
            // instead of regenerating; unapproved candidates are not reused.
            if stage == "design" {
                if let Some(rev) = self.artifacts.identity()?.design_revision.clone() {
                    if self.artifacts.approval_valid("design", &rev)? {
                        self.state.design_revision = Some(rev.clone());
                        self.state.stages.insert(stage.clone(), StageStatus::Succeeded);
                        self.run.save(&self.state)?;
                        self.events.append(Event::StageStatus {
                            stage: stage.clone(),
                            status: "succeeded".into(),
                        })?;
                        continue;
                    }
                    return Err(HarnessError::blocked(
                        "design revision exists but its approval is invalid",
                    )
                    .with_hint("pxl-harness approve --stage design --revision <rev>"));
                }
            }
            let status = match stage.as_str() {
                "design" => self.run_design(provider)?,
                "assets" => self.run_assets(provider)?,
                other => {
                    return Err(HarnessError::semantic(format!(
                        "unknown stage '{other}'"
                    )))
                }
            };
            self.events.append(Event::StageStatus {
                stage: stage.clone(),
                status: status.as_str().into(),
            })?;
            match status {
                StageStatus::Succeeded => continue,
                StageStatus::WaitingApproval => {
                    return Ok(RunSummary {
                        run_id: self.state.run_id.clone(),
                        status: "waiting_approval".into(),
                        exit_code: exit::WAITING_APPROVAL,
                    })
                }
                StageStatus::Failed => {
                    return Ok(RunSummary {
                        run_id: self.state.run_id.clone(),
                        status: "failed".into(),
                        exit_code: exit::FAILED,
                    })
                }
                StageStatus::BudgetExhausted => {
                    return Ok(RunSummary {
                        run_id: self.state.run_id.clone(),
                        status: "budget_exhausted".into(),
                        exit_code: exit::BUDGET_EXHAUSTED,
                    })
                }
                other => {
                    return Ok(RunSummary {
                        run_id: self.state.run_id.clone(),
                        status: other.as_str().into(),
                        exit_code: exit::BLOCKED,
                    })
                }
            }
        }
        Ok(RunSummary {
            run_id: self.state.run_id.clone(),
            status: "succeeded".into(),
            exit_code: exit::OK,
        })
    }

    fn run_design(&mut self, provider: &mut dyn ModelProvider) -> Result<StageStatus> {
        // Project-local templates override; the harness-bundled genre
        // templates are the fallback for every project.
        let template_root = self.project.join("templates");
        std::fs::create_dir_all(&template_root)?;
        let bundled_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let roots: Vec<PathBuf> = vec![
            template_root,
            bundled_root,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../templates"),
        ];
        let status = crate::pipeline::design_stage::run_design_stage(
            provider,
            &self.cfg,
            &self.artifacts,
            &self.run,
            &mut self.state,
            &mut self.ledger,
            &mut self.events,
            &roots,
        );
        self.ledger.save(self.run.dir())?;
        self.run.save(&self.state)?;
        status
    }

    fn run_assets(&mut self, provider: &mut dyn ModelProvider) -> Result<StageStatus> {
        let template_root = self.project.join("templates");
        std::fs::create_dir_all(&template_root)?;
        let bundled_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let roots = vec![
            template_root,
            bundled_root,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../templates"),
        ];
        let pxl = std::sync::Arc::new(crate::adapters::PxlAdapter::discover(&self.cfg)?);
        let status = crate::pipeline::assets_stage::run_assets_stage(
            provider,
            &self.cfg,
            &self.artifacts,
            &self.run,
            &mut self.state,
            &mut self.ledger,
            &mut self.events,
            &roots,
            pxl,
        );
        self.ledger.save(self.run.dir())?;
        self.run.save(&self.state)?;
        status
    }

    pub fn summary(&self) -> RunSummary {
        let exit_code = match self.state.status.as_str() {
            "succeeded" => exit::OK,
            "waiting_approval" => exit::WAITING_APPROVAL,
            "budget_exhausted" => exit::BUDGET_EXHAUSTED,
            "failed" => exit::FAILED,
            _ => exit::BLOCKED,
        };
        RunSummary {
            run_id: self.state.run_id.clone(),
            status: self.state.status.clone(),
            exit_code,
        }
    }

    pub fn cfg(&self) -> &Config {
        &self.cfg
    }

    pub fn state(&self) -> &RunState {
        &self.state
    }

    pub fn run_dir(&self) -> &Path {
        self.run.dir()
    }
}

/// Build the concrete provider declared by the config. Credentials stay in
/// the environment; the fake provider is fully offline.
pub fn provider_from_config(cfg: &Config) -> Result<Box<dyn ModelProvider>> {
    match cfg.provider {
        ProviderKind::Fake => {
            if let Ok(script) = std::env::var("PXL_HARNESS_FAKE_TURNS") {
                if !script.is_empty() {
                    return Ok(Box::new(FakeProvider::from_scripted_json(&script)?));
                }
            }
            Ok(Box::new(FakeProvider::new(Vec::new())))
        }
        ProviderKind::Openai => Ok(Box::new(OpenAiProvider::from_dotenv(
            cfg,
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.env"),
        )?)),
    }
}

fn stages_up_to(until: UntilStage) -> Vec<String> {
    let stop = STAGES
        .iter()
        .position(|s| *s == until.as_str())
        .unwrap_or(0);
    STAGES[..=stop]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn timestamp_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Machine-readable result printed to stdout at the end of a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: String,
    pub status: String,
    pub exit_code: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn until_parsing_and_stage_order() {
        assert!(UntilStage::parse("design").is_ok());
        assert!(UntilStage::parse("assets").is_ok());
        assert!(UntilStage::parse("nope").is_err());
        assert_eq!(
            stages_up_to(UntilStage::Design),
            vec!["design".to_string()]
        );
        assert_eq!(
            stages_up_to(UntilStage::Assets),
            vec!["design".to_string(), "assets".to_string()]
        );
    }
}
