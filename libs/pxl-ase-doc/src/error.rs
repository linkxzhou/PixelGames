use alloc::string::String;
use core::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AseError {
    pub message: String,
    pub hint: Option<String>,
}

impl AseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for AseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(h) = &self.hint {
            write!(f, " ({h})")?;
        }
        Ok(())
    }
}

pub type AseResult<T> = Result<T, AseError>;
