//! Unified error type and the frozen exit-code table.
//!
//! Exit codes (frozen for H0, distinct from `pxl` exit codes 1–6 which are
//! only ever reported inside structured tool results):
//!
//! | code | meaning                                        |
//! |------|------------------------------------------------|
//! | 0    | command succeeded / stage reached requested end |
//! | 1    | IO or internal error                            |
//! | 2    | usage error                                     |
//! | 3    | schema error in a harness contract              |
//! | 4    | semantic error (not found, exists, bad ref)     |
//! | 10   | waiting for approval                            |
//! | 11   | blocked (needs input, policy, unsupported)      |
//! | 12   | stage failed (gates did not pass)               |
//! | 13   | budget exhausted                                |
//! | 14   | project lock held by another process            |
//! | 15   | integrity error (corrupt log, digest mismatch)  |
//! | 16   | tool / provider infrastructure error            |

use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    Io,
    Usage,
    Schema,
    Semantic,
    NotFound,
    Exists,
    WaitingApproval,
    Blocked,
    Policy,
    Gate,
    Budget,
    Lock,
    Integrity,
    Tool,
    Provider,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::Io => "E_IO",
            ErrorCode::Usage => "E_USAGE",
            ErrorCode::Schema => "E_SCHEMA",
            ErrorCode::Semantic => "E_SEMANTIC",
            ErrorCode::NotFound => "E_NOT_FOUND",
            ErrorCode::Exists => "E_EXISTS",
            ErrorCode::WaitingApproval => "E_WAITING_APPROVAL",
            ErrorCode::Blocked => "E_BLOCKED",
            ErrorCode::Policy => "E_POLICY",
            ErrorCode::Gate => "E_GATE",
            ErrorCode::Budget => "E_BUDGET",
            ErrorCode::Lock => "E_LOCK",
            ErrorCode::Integrity => "E_INTEGRITY",
            ErrorCode::Tool => "E_TOOL",
            ErrorCode::Provider => "E_PROVIDER",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            ErrorCode::Io => 1,
            ErrorCode::Usage => 2,
            ErrorCode::Schema => 3,
            ErrorCode::Semantic | ErrorCode::NotFound | ErrorCode::Exists => 4,
            ErrorCode::WaitingApproval => 10,
            ErrorCode::Blocked | ErrorCode::Policy => 11,
            ErrorCode::Gate => 12,
            ErrorCode::Budget => 13,
            ErrorCode::Lock => 14,
            ErrorCode::Integrity => 15,
            ErrorCode::Tool | ErrorCode::Provider => 16,
        }
    }
}

pub mod exit {
    pub const OK: i32 = 0;
    pub const WAITING_APPROVAL: i32 = 10;
    pub const BLOCKED: i32 = 11;
    pub const FAILED: i32 = 12;
    pub const BUDGET_EXHAUSTED: i32 = 13;
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl HarnessError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
        }
    }
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn io(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, m)
    }
    pub fn usage(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, m)
    }
    pub fn schema(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Schema, m)
    }
    pub fn semantic(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Semantic, m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, m)
    }
    pub fn exists(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Exists, m)
    }
    pub fn policy(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Policy, m)
    }
    pub fn blocked(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Blocked, m)
    }
    pub fn integrity(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Integrity, m)
    }
    pub fn tool(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Tool, m)
    }
    pub fn provider(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Provider, m)
    }
    pub fn lock(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Lock, m)
    }
    pub fn budget(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Budget, m)
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "code": self.code.as_str(),
            "message": self.message,
        });
        if let Some(h) = &self.hint {
            v["hint"] = serde_json::Value::String(h.clone());
        }
        v
    }
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for HarnessError {}

impl From<std::io::Error> for HarnessError {
    fn from(e: std::io::Error) -> Self {
        HarnessError::io(e.to_string())
    }
}

impl From<serde_json::Error> for HarnessError {
    fn from(e: serde_json::Error) -> Self {
        HarnessError::schema(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, HarnessError>;
