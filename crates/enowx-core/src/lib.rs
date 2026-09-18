//! Agent core: configuration, model streaming, the tool-calling loop, the
//! built-in tool surface, session persistence, and the three shipped roles.

pub mod agent;
pub mod agent_def;
pub mod catalog;
pub mod compact;
pub mod config;
pub mod discovery;
pub mod event;
pub mod format;
pub mod mcp;
pub mod message;
pub mod persist;
pub mod routing;
pub mod provider;
pub mod role;
pub mod session;
pub mod tools;

pub use agent::{Agent, RunRequest};
pub use agent_def::{builtin_agents, AgentDef, Delegation, Tier};
pub use routing::{Delegation as DelegationRequest, Refusal, Switch};
pub use config::{Config, UpstreamModel};
pub use discovery::{Discovery, InstructionFile, McpServer, McpTransport, SkillEntry, SkillScope};
pub use event::Event;
pub use mcp::{McpClient, McpTool};
pub use message::{Message, Role as MessageRole, ToolCall};
pub use provider::{provider_preset, ProviderPreset, PROVIDER_PRESETS};
pub use role::{Role, ROLES};
pub use session::{Session, SessionStore, StoredTurn};
pub use tools::{Tool, ToolCtx, ToolOutput, ToolRegistry};
