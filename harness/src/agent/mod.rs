//! Agent kernel: providers, tools, roles, loop.

pub mod provider;
pub mod role;
pub mod runner;
pub mod tools;

pub use provider::{ChatMessage, FakeProvider, FakeTurn, ModelProvider, ProviderResponse, ToolCall, ToolSpec};
pub use role::{artist, designer, RoleProfile};
pub use runner::{AgentRunner, AgentTask, LoopResult};
pub use tools::{ToolContext, ToolDef, ToolOutcome, ToolRegistry};
