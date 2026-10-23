//! Agent kernel: providers, tools, roles, loop.

pub mod compress;
pub mod goal;
pub mod mcp;
pub mod provider;
pub mod role;
pub mod runner;
pub mod tools;

pub use goal::{GoalEvaluator, GoalRecord, GoalRuntime};
pub use provider::{ChatMessage, FakeProvider, FakeTurn, ModelProvider, ProviderResponse, ToolCall, ToolSpec};
pub use role::{artist, artist_for, designer, designer_for, PromptLang, RoleProfile};
pub use runner::{AgentRunner, AgentTask, LoopResult};
pub use tools::{Tool, ToolContext, ToolDef, ToolOutcome, ToolRegistry, ToolSource};
