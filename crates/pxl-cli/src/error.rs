use pxl_core::{ErrorCode, PxlError};
use serde_json::Value;

#[derive(Debug)]
pub struct CmdError {
    pub err: PxlError,
    pub extra: Option<Value>,
}

impl CmdError {
    pub fn new(err: PxlError) -> Self {
        Self { err, extra: None }
    }

    pub fn code(&self) -> ErrorCode {
        self.err.code
    }

    pub fn with_extra(mut self, extra: Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

impl From<PxlError> for CmdError {
    fn from(err: PxlError) -> Self {
        Self::new(err)
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
