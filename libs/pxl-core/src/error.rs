use alloc::string::String;

pub type PxlResult<T> = Result<T, PxlError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    Io,
    Usage,
    Schema,
    Semantic,
    Qa,
    Exists,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Io => "E_IO",
            ErrorCode::Usage => "E_USAGE",
            ErrorCode::Schema => "E_SCHEMA",
            ErrorCode::Semantic => "E_SEMANTIC",
            ErrorCode::Qa => "E_QA",
            ErrorCode::Exists => "E_EXISTS",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            ErrorCode::Io => 1,
            ErrorCode::Usage => 2,
            ErrorCode::Schema => 3,
            ErrorCode::Semantic => 4,
            ErrorCode::Qa => 5,
            ErrorCode::Exists => 6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PxlError {
    pub code: ErrorCode,
    pub message: String,
    pub path: Option<String>,
    pub hint: Option<String>,
}

impl PxlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
            hint: None,
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, message)
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message)
    }

    pub fn schema(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Schema, message)
    }

    pub fn semantic(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Semantic, message)
    }

    pub fn qa(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Qa, message)
    }

    pub fn exists(path: &str) -> Self {
        Self::new(
            ErrorCode::Exists,
            alloc::format!("output already exists: {path}"),
        )
        .with_hint("pass --overwrite to replace it, or choose a new -o path")
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
