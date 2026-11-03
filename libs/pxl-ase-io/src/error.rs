use std::fmt;

#[derive(Debug)]
pub struct IoError {
    pub message: String,
    pub hint: Option<String>,
}

impl IoError {
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

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(h) = &self.hint {
            write!(f, " ({h})")?;
        }
        Ok(())
    }
}

impl std::error::Error for IoError {}

impl From<pxl_ase_doc::AseError> for IoError {
    fn from(e: pxl_ase_doc::AseError) -> Self {
        let mut err = IoError::new(e.message);
        if let Some(h) = e.hint {
            err = err.with_hint(h);
        }
        err
    }
}

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        IoError::new(format!("io: {e}"))
    }
}

pub type IoResult<T> = Result<T, IoError>;
