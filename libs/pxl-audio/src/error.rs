use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioErrorKind {
    /// The document does not match the schema (unknown field, wrong type).
    Schema,
    /// The document is well formed but not valid audio (bad range, bad note).
    Semantic,
    /// A frozen limit was exceeded.
    Limit,
    Io,
    /// A required tool (Node, the adapter) is missing or too old.
    Blocked,
    /// The adapter ran and failed.
    Backend,
    /// The render finished but a hard QA check failed.
    Qa,
    Exists,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioError {
    pub kind: AudioErrorKind,
    pub message: String,
    pub path: Option<String>,
    pub hint: Option<String>,
}

pub type AudioResult<T> = Result<T, AudioError>;

impl AudioError {
    pub fn new(kind: AudioErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            path: None,
            hint: None,
        }
    }

    pub fn schema(message: impl Into<String>) -> Self {
        Self::new(AudioErrorKind::Schema, message)
    }

    pub fn semantic(message: impl Into<String>) -> Self {
        Self::new(AudioErrorKind::Semantic, message)
    }

    pub fn limit(message: impl Into<String>) -> Self {
        Self::new(AudioErrorKind::Limit, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(AudioErrorKind::Io, message)
    }

    pub fn blocked(message: impl Into<String>) -> Self {
        Self::new(AudioErrorKind::Blocked, message)
    }

    pub fn backend(message: impl Into<String>) -> Self {
        Self::new(AudioErrorKind::Backend, message)
    }

    pub fn qa(message: impl Into<String>) -> Self {
        Self::new(AudioErrorKind::Qa, message)
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AudioError {}
