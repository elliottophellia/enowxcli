//! Agent core: configuration, model streaming, the tool-calling loop, the
//! built-in tool surface, session persistence, and the three shipped roles.

pub mod acp;
pub mod agent;
pub mod agent_def;
pub mod ask;
pub mod auth;
pub mod builtin_mcp;
pub mod catalog;
pub mod comms;
pub mod compact;
pub mod config;
pub mod contract;
pub mod dashes;
pub mod decision;
pub mod discovery;
pub mod eval;
pub mod event;
pub mod format;
pub mod lsp;
pub mod mcp;
pub mod message;
pub mod model_state;
pub mod persist;
pub mod preview;
pub mod provider;
pub mod role;
pub mod routing;
pub mod rules;
pub mod session;
pub mod skill_bindings;
pub mod syntax;
pub mod tools;
pub mod ui_check;
pub mod update;

pub use agent::{Agent, RunRequest};
pub use agent_def::{builtin_agents, AgentDef, Delegation, Tier};
pub use config::Config;
pub use discovery::{Discovery, InstructionFile, McpServer, McpTransport, SkillEntry, SkillScope};
pub use event::Event;
pub use mcp::{McpClient, McpTool};
pub use message::{Message, Role as MessageRole, ToolCall};
pub use provider::registry::{Connection, ModelFacts, ModelRef};
pub use provider::{
    classify, provider_preset, retry_budget_for_error, tier_drop, FailureKind, LadderStep,
    ModelLadder, ProviderPreset, TierDrop, TierNoticeSink, PROVIDER_PRESETS,
};
pub use role::{Role, ROLES};
pub use routing::{Delegation as DelegationRequest, Refusal, Switch};
pub use session::{DelegationRecord, Session, SessionStore, StoredTurn};
pub use tools::{Tool, ToolCtx, ToolOutput, ToolRegistry};
