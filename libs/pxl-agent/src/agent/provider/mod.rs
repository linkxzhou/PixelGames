//! Model providers (OpenAI-compatible + fake).
mod fake;
mod openai;
mod types;

pub use fake::FakeProvider;
pub use fake::FakeTurn;
pub use openai::OpenAiProvider;
pub use types::*;
pub use openai::tool_calls_value;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
