//! Modal pickers and the provider/model settings draft they edit.

pub use enowx_core::persist::McpDraft;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    /// The agent roster as a picker.
    Agents,
    /// Actions on a sent message: edit, resend, copy.
    Message,
    /// Editing a sent message before resending it.
    MessageEdit,
    Sessions,
    /// Every provider enx knows, connected or not, and a row to add one.
    Providers,
    /// The API key of a built-in provider.
    ProviderKey,
    /// A custom provider's name, endpoint, key and model-list URL.
    ProviderForm,
    /// The models of every connected provider, with the recent and
    /// favourite ones first.
    Models,
    /// A model entered by hand, for a provider whose list lacks it.
    ModelManual,
    /// Editing a known model's properties: context window, thinking effort,
    /// vision and prices, over what the catalogue or the id says.
    ModelEdit,
    Themes,
    /// The thinking efforts the model in use offers.
    Effort,
    Attach,
    /// Floating list of every command, searchable. Distinct from the
    /// inline list that appears above the composer when the input starts
    /// with `/`: that one is for people who know the name they want, this is
    /// for looking.
    Commands,
    Skills,
    Mcp,
    McpForm,
    /// Configure a built-in MCP server (coolify, dokploy, or a VPS).
    BuiltinMcp,
    /// Settings > RAG: code search on or off, its database, and where its
    /// embeddings come from.
    Rag,
    /// Settings > Team: agents messaging each other, a shared board, and
    /// cross-review.
    Team,
    /// Settings > Updates: the check at start, and installing by itself.
    Updates,
    /// Settings > General: how the agents work (preview, language server,
    /// background delegation, compaction, limits, the model per tier).
    General,
    /// Settings > Display: the sidebar and the currency prices are shown in.
    Display,
    /// Ctrl+C in an empty composer: confirm before quitting.
    QuitConfirm,
    /// Right-click on a sub-agent at work: confirm before stopping it.
    StopConfirm,
    /// `/handoff`: carry on in a fresh session, keeping or deleting this
    /// one's history.
    Handoff,
    /// Settings > Decision model.
    Decision,
    /// Settings > ACP agents: one card per agent.
    Acp,
    /// One ACP agent's card: status, model, effort, permissions.
    AcpEngine,
    /// A custom ACP agent's command, arguments and environment.
    AcpCustom,
    /// `/acp`: pick the agent the session runs on.
    AcpPick,
    /// `/model` while an ACP agent runs the session: its models.
    AcpModels,
    /// `/effort` while an ACP agent runs the session: its efforts.
    AcpEfforts,
}

impl Modal {
    pub fn title(self) -> &'static str {
        match self {
            Modal::Commands => " COMMANDS ",
            Modal::Agents => " AGENT ",
            Modal::Message => " MESSAGE ",
            Modal::MessageEdit => " EDIT PROMPT ",
            Modal::Sessions => " RESUME SESSION ",
            Modal::Providers => " PROVIDERS ",
            Modal::Models => " MODELS ",
            Modal::Themes => " THEME ",
            Modal::Effort => " THINKING EFFORT ",
            Modal::Attach => " ATTACH IMAGE ",
            Modal::Skills => " SKILLS ",
            Modal::Mcp => " MCP SERVERS ",
            Modal::McpForm => " ADD MCP SERVER ",
            Modal::QuitConfirm => " QUIT ENX ",
            Modal::StopConfirm => " STOP SUB-AGENT ",
            Modal::Handoff => " HAND OFF TO A NEW SESSION ",
            // Forms draw their own heading, so the generic title is empty.
            Modal::None
            | Modal::ProviderForm
            | Modal::ModelManual
            | Modal::ModelEdit
            | Modal::BuiltinMcp
            | Modal::Rag
            | Modal::Team
            | Modal::Decision
            | Modal::AcpEngine
            | Modal::AcpCustom
            | Modal::Updates
            | Modal::General
            | Modal::Display
            | Modal::ProviderKey => "",
            Modal::Acp => " ACP AGENTS ",
            Modal::AcpPick => " RUN THIS SESSION ON ",
            Modal::AcpModels => " MODEL ",
            Modal::AcpEfforts => " THINKING EFFORT ",
        }
    }

    /// Text-field modals share one key handler and one renderer.
    pub fn is_form(self) -> bool {
        matches!(
            self,
            Modal::ProviderForm
                | Modal::ModelManual
                | Modal::ModelEdit
                | Modal::ProviderKey
                | Modal::McpForm
                | Modal::BuiltinMcp
                | Modal::Rag
                | Modal::Team
                | Modal::Decision
                | Modal::AcpEngine
                | Modal::AcpCustom
                | Modal::Updates
                | Modal::General
                | Modal::Display
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsField {
    Name,
    BaseUrl,
    ApiKey,
    ModelsUrl,
    Model,
    ContextWindow,
    Effort,
    Vision,
    PriceInput,
    PriceOutput,
    Host,
    User,
    Port,
    /// A Postgres connection string. Masked: it carries the password.
    Dsn,
    /// A VPS's private key file.
    KeyFile,
    /// That key file's passphrase. Masked.
    Passphrase,
    /// A VPS's SSH password. Masked; kept in the `api_key` draft.
    Password,
    /// RAG on or off.
    RagEnabled,
    /// Where RAG's embeddings come from: Voyage, OpenAI, or a custom
    /// OpenAI-compatible endpoint.
    RagProvider,
    /// The embedding model, picked from the provider's.
    EmbedModel,
    /// The embedding model, typed: a custom endpoint's.
    EmbedModelText,
    /// The vector width, picked from what the model offers.
    Dimension,
    /// The vector width, typed.
    DimensionText,
    /// The reranker, picked (or off).
    Rerank,
    /// The reranker, typed; blank for none.
    RerankText,
    /// A custom endpoint's base URL. Kept in the `base_url` draft.
    EmbedUrl,
    /// Keep the RAG index fresh by itself, on or off.
    AutoIndex,
    /// Settings > Team, every one a choice.
    TeamEnabled,
    TeamMessages,
    TeamBoard,
    TeamReview,
    ReviewRounds,
    Reviewer,
    /// Settings > Updates.
    UpdateCheck,
    UpdateAuto,
    /// A config key, by its place in `CONF_FIELDS`: General and Display.
    Conf(usize),
    /// Settings > Decision model.
    Dec(DecField),
    /// Settings > ACP agents.
    Acp(AcpField),
}

/// The fields of an ACP agent's card and of the custom agent form.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AcpField {
    /// What is installed and signed in. Enter installs the adapter, or
    /// checks again and reads the agent's models.
    Status,
    Model,
    Effort,
    Permission,
    /// Enter runs this session on the agent.
    Use,
    /// Enter edits a custom agent's command.
    Edit,
    CustomName,
    CustomCommand,
    CustomArgs,
    CustomEnv,
    CustomPermission,
}

impl AcpField {
    /// Its slot in the draft.
    pub fn key(self) -> String {
        match self {
            AcpField::Status => "status",
            AcpField::Model => "model",
            AcpField::Effort => "effort",
            AcpField::Permission | AcpField::CustomPermission => "permission",
            AcpField::Use => "use",
            AcpField::Edit => "edit",
            AcpField::CustomName => "name",
            AcpField::CustomCommand => "command",
            AcpField::CustomArgs => "args",
            AcpField::CustomEnv => "env",
        }
        .into()
    }

    fn label(self) -> &'static str {
        match self {
            AcpField::Status => "Status",
            AcpField::Model => "Default model",
            AcpField::Effort => "Thinking effort",
            AcpField::Permission | AcpField::CustomPermission => "Permissions",
            AcpField::Use => "Run this session on it",
            AcpField::Edit => "Command, arguments and environment",
            AcpField::CustomName => "Name",
            AcpField::CustomCommand => "Command",
            AcpField::CustomArgs => "Arguments",
            AcpField::CustomEnv => "Environment (KEY=value; KEY2=value)",
        }
    }
}

/// Efforts offered before the agent has said which it has.
pub const ACP_EFFORTS: [&str; 6] = ["", "low", "medium", "high", "xhigh", "max"];

/// A built-in agent's card.
pub const ACP_CARD: [SettingsField; 5] = [
    SettingsField::Acp(AcpField::Status),
    SettingsField::Acp(AcpField::Model),
    SettingsField::Acp(AcpField::Effort),
    SettingsField::Acp(AcpField::Permission),
    SettingsField::Acp(AcpField::Use),
];

/// A custom agent's card: the same, and its command.
pub const ACP_CARD_CUSTOM: [SettingsField; 6] = [
    SettingsField::Acp(AcpField::Status),
    SettingsField::Acp(AcpField::Model),
    SettingsField::Acp(AcpField::Effort),
    SettingsField::Acp(AcpField::Permission),
    SettingsField::Acp(AcpField::Use),
    SettingsField::Acp(AcpField::Edit),
];

/// Adding or editing a custom agent.
pub const ACP_CUSTOM: [SettingsField; 5] = [
    SettingsField::Acp(AcpField::CustomName),
    SettingsField::Acp(AcpField::CustomCommand),
    SettingsField::Acp(AcpField::CustomArgs),
    SettingsField::Acp(AcpField::CustomEnv),
    SettingsField::Acp(AcpField::CustomPermission),
];

/// The fields of Settings > Decision model.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecField {
    Enabled,
    Provider,
    /// A built-in provider's model, picked.
    ModelPick,
    /// A custom endpoint's model, or the enowx model to answer with, typed.
    ModelText,
    Account,
    BaseUrl,
    Key,
    Timeout,
    Shadow,
    /// Enter here saves, then asks the provider a test question.
    Test,
    UseOn(Use),
    Threshold(Use),
}

use enowx_core::decision::Use;

impl DecField {
    /// Its slot in the draft.
    pub fn key(self) -> &'static str {
        match self {
            DecField::Enabled => "enabled",
            DecField::Provider => "provider",
            DecField::ModelPick | DecField::ModelText => "model",
            DecField::Account => "account",
            DecField::BaseUrl => "base_url",
            DecField::Key => "key",
            DecField::Timeout => "timeout",
            DecField::Shadow => "shadow",
            DecField::Test => "test",
            DecField::UseOn(u) => match u {
                Use::Intent => "on.intent",
                Use::Routing => "on.routing",
                Use::Ask => "on.ask",
                Use::ToolResults => "on.tool_results",
                Use::Shell => "on.shell",
            },
            DecField::Threshold(u) => match u {
                Use::Intent => "th.intent",
                Use::Routing => "th.routing",
                Use::Ask => "th.ask",
                Use::ToolResults => "th.tool_results",
                Use::Shell => "th.shell",
            },
        }
    }

    fn label(self) -> &'static str {
        match self {
            DecField::Enabled => "Decision model",
            DecField::Provider => "Provider",
            DecField::ModelPick | DecField::ModelText => "Model",
            DecField::Account => "Cloudflare account ID",
            DecField::BaseUrl => "Endpoint URL (speaks the Jev/Clef API)",
            DecField::Key => "API key or token",
            DecField::Timeout => "Give up after, then the usual path",
            DecField::Shadow => "Shadow mode: decide and record, change nothing",
            DecField::Test => "Test the connection",
            DecField::UseOn(u) => u.label(),
            DecField::Threshold(_) => "  sure enough at",
        }
    }
}

/// Timeouts offered, in milliseconds.
pub const DECISION_TIMEOUTS: [&str; 8] =
    ["150", "300", "500", "1000", "2000", "3000", "5000", "10000"];

/// Thresholds offered.
pub const DECISION_THRESHOLDS: [&str; 8] =
    ["0.6", "0.7", "0.75", "0.8", "0.85", "0.9", "0.95", "0.99"];

macro_rules! decision_fields {
    ($($middle:expr),*) => {
        [
            SettingsField::Dec(DecField::Enabled),
            SettingsField::Dec(DecField::Provider),
            $(SettingsField::Dec($middle),)*
            SettingsField::Dec(DecField::Timeout),
            SettingsField::Dec(DecField::Shadow),
            SettingsField::Dec(DecField::Test),
            SettingsField::Dec(DecField::UseOn(Use::Intent)),
            SettingsField::Dec(DecField::Threshold(Use::Intent)),
            SettingsField::Dec(DecField::UseOn(Use::Routing)),
            SettingsField::Dec(DecField::Threshold(Use::Routing)),
            SettingsField::Dec(DecField::UseOn(Use::Ask)),
            SettingsField::Dec(DecField::Threshold(Use::Ask)),
            SettingsField::Dec(DecField::UseOn(Use::ToolResults)),
            SettingsField::Dec(DecField::Threshold(Use::ToolResults)),
            SettingsField::Dec(DecField::UseOn(Use::Shell)),
            SettingsField::Dec(DecField::Threshold(Use::Shell)),
        ]
    };
}

const DECISION_CLEF: [SettingsField; 18] =
    decision_fields!(DecField::ModelPick, DecField::Account, DecField::Key);
const DECISION_JEV: [SettingsField; 17] = decision_fields!(DecField::ModelPick, DecField::Key);
const DECISION_CUSTOM: [SettingsField; 18] =
    decision_fields!(DecField::BaseUrl, DecField::ModelText, DecField::Key);
const DECISION_LLM: [SettingsField; 16] = decision_fields!(DecField::ModelText);

/// The fields of Settings > Decision model for a provider: each asks for
/// what its provider needs.
pub fn decision_fields(provider: &str) -> &'static [SettingsField] {
    match provider {
        "jev" => &DECISION_JEV,
        "custom" => &DECISION_CUSTOM,
        "llm" => &DECISION_LLM,
        _ => &DECISION_CLEF,
    }
}

/// How a config field is edited.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConfKind {
    /// On or off.
    Bool,
    /// One of these values, cycled with Left/Right.
    Choice(&'static [&'static str]),
    /// Typed.
    Text,
}

/// The config keys General and Display edit: the dotted key `Config::set`
/// takes, its label, how it is edited, and a placeholder for text.
pub const CONF_FIELDS: [(&str, &str, ConfKind, &str); 15] = [
    (
        "agent.preview",
        "Look at pages in a browser (preview)",
        ConfKind::Bool,
        "",
    ),
    (
        "agent.lsp",
        "Check edits with the language server",
        ConfKind::Bool,
        "",
    ),
    (
        "agent.background_delegation",
        "Delegations run in the background",
        ConfKind::Bool,
        "",
    ),
    (
        "agent.auto_switch",
        "Switch agent by itself",
        ConfKind::Bool,
        "",
    ),
    (
        "agent.auto_compact",
        "Compact the context by itself",
        ConfKind::Bool,
        "",
    ),
    (
        "agent.auto_compact_at",
        "Compact when the context is this full",
        ConfKind::Choice(&["0.5", "0.6", "0.7", "0.75", "0.8", "0.85", "0.9", "0.95"]),
        "",
    ),
    (
        "agent.compact_keep_last",
        "Turns kept as they were when compacting",
        ConfKind::Choice(&["2", "4", "6", "8", "12"]),
        "",
    ),
    (
        "agent.max_steps",
        "Model calls per turn at most",
        ConfKind::Text,
        "0 for no limit",
    ),
    (
        "agent.shell_timeout_secs",
        "Seconds a shell command may run",
        ConfKind::Text,
        "120",
    ),
    (
        "agent.tiers.cheap",
        "Model for cheap work",
        ConfKind::Text,
        "provider/model (blank: the one in use)",
    ),
    (
        "agent.tiers.balanced",
        "Model for balanced work",
        ConfKind::Text,
        "provider/model (blank: the one in use)",
    ),
    (
        "agent.tiers.strong",
        "Model for strong work",
        ConfKind::Text,
        "provider/model (blank: the one in use)",
    ),
    ("ui.show_sidebar", "Show the sidebar", ConfKind::Bool, ""),
    (
        "ui.currency",
        "Currency shown beside prices",
        ConfKind::Text,
        "USD, IDR, EUR...",
    ),
    (
        "ui.currency_rate",
        "Rate from US dollars",
        ConfKind::Text,
        "1 for USD; 15800 for IDR",
    ),
];

/// Settings > General's fields.
pub const GENERAL_FIELDS: [SettingsField; 12] = [
    SettingsField::Conf(0),
    SettingsField::Conf(1),
    SettingsField::Conf(2),
    SettingsField::Conf(3),
    SettingsField::Conf(4),
    SettingsField::Conf(5),
    SettingsField::Conf(6),
    SettingsField::Conf(7),
    SettingsField::Conf(8),
    SettingsField::Conf(9),
    SettingsField::Conf(10),
    SettingsField::Conf(11),
];

/// Settings > Display's fields.
pub const DISPLAY_FIELDS: [SettingsField; 3] = [
    SettingsField::Conf(12),
    SettingsField::Conf(13),
    SettingsField::Conf(14),
];

impl SettingsField {
    pub fn label(self) -> &'static str {
        match self {
            SettingsField::Name => "Name",
            SettingsField::BaseUrl => "Base URL",
            SettingsField::ApiKey => "API key",
            SettingsField::ModelsUrl => "Model-list URL",
            SettingsField::Model => "Model ID",
            SettingsField::ContextWindow => "Context window (tokens)",
            SettingsField::Effort => "Thinking effort",
            SettingsField::Vision => "Vision (sees images)",
            SettingsField::PriceInput => "Price in ($/1M tokens)",
            SettingsField::PriceOutput => "Price out ($/1M tokens)",
            SettingsField::Host => "Host (address)",
            SettingsField::User => "SSH user",
            SettingsField::Port => "SSH port",
            SettingsField::Dsn => "Database (Postgres with pgvector)",
            SettingsField::KeyFile => "Key file (optional)",
            SettingsField::Passphrase => "Key passphrase (if it has one)",
            SettingsField::Password => "Password (optional)",
            SettingsField::RagEnabled => "Code search (RAG)",
            SettingsField::RagProvider => "Embedding provider",
            SettingsField::EmbedModel | SettingsField::EmbedModelText => "Embedding model",
            SettingsField::Dimension | SettingsField::DimensionText => "Dimension (vector width)",
            SettingsField::Rerank | SettingsField::RerankText => "Reranker",
            SettingsField::EmbedUrl => "Base URL (OpenAI-compatible)",
            SettingsField::AutoIndex => "Index automatically (on start and on change)",
            SettingsField::TeamEnabled => "Agents work together",
            SettingsField::TeamMessages => "Messages between agents at work",
            SettingsField::TeamBoard => "Shared board for each run",
            SettingsField::TeamReview => "Cross-review of delegated work",
            SettingsField::ReviewRounds => "Correction rounds at most",
            SettingsField::Reviewer => "Reviewer",
            SettingsField::UpdateCheck => "Check for a new release at start",
            SettingsField::UpdateAuto => "Install it by itself (used from the next start)",
            SettingsField::Conf(i) => CONF_FIELDS.get(i).map_or("", |f| f.1),
            SettingsField::Dec(field) => field.label(),
            SettingsField::Acp(field) => field.label(),
        }
    }

    /// Typed hidden, shown as dots: a key, a password, a connection string.
    pub fn is_secret(self) -> bool {
        matches!(
            self,
            SettingsField::ApiKey
                | SettingsField::Dsn
                | SettingsField::Passphrase
                | SettingsField::Password
                | SettingsField::Dec(DecField::Key)
        )
    }

    /// A cycled choice (Left/Right picks a value) rather than a text field.
    /// Its draft string holds the chosen value or is empty for the default.
    pub fn is_choice(self) -> bool {
        matches!(
            self,
            SettingsField::Effort
                | SettingsField::Vision
                | SettingsField::RagEnabled
                | SettingsField::AutoIndex
                | SettingsField::TeamEnabled
                | SettingsField::TeamMessages
                | SettingsField::TeamBoard
                | SettingsField::TeamReview
                | SettingsField::ReviewRounds
                | SettingsField::Reviewer
                | SettingsField::UpdateCheck
                | SettingsField::UpdateAuto
                | SettingsField::RagProvider
                | SettingsField::EmbedModel
                | SettingsField::Dimension
                | SettingsField::Rerank
        ) || matches!(
            self,
            SettingsField::Conf(i) if CONF_FIELDS.get(i).is_some_and(|f| f.2 != ConfKind::Text)
        ) || matches!(
            self,
            SettingsField::Dec(
                DecField::Enabled
                    | DecField::Provider
                    | DecField::ModelPick
                    | DecField::Timeout
                    | DecField::Shadow
                    | DecField::Test
                    | DecField::UseOn(_)
                    | DecField::Threshold(_)
            )
        ) || matches!(
            self,
            SettingsField::Acp(
                AcpField::Status
                    | AcpField::Model
                    | AcpField::Effort
                    | AcpField::Permission
                    | AcpField::CustomPermission
                    | AcpField::Use
                    | AcpField::Edit
            )
        )
    }

    /// A button: Enter acts on it, and it shows a line of status rather
    /// than a value.
    pub fn is_action(self) -> bool {
        matches!(
            self,
            SettingsField::Dec(DecField::Test)
                | SettingsField::Acp(AcpField::Status | AcpField::Use | AcpField::Edit)
        )
    }
}

/// The fields each form shows, in order. `modal_cursor` indexes this list.
pub fn form_fields(modal: Modal) -> &'static [SettingsField] {
    match modal {
        Modal::ProviderForm => &[
            SettingsField::Name,
            SettingsField::BaseUrl,
            SettingsField::ApiKey,
            SettingsField::ModelsUrl,
        ],
        Modal::ModelManual => &[SettingsField::Model, SettingsField::ContextWindow],
        Modal::ModelEdit => &[
            SettingsField::ContextWindow,
            SettingsField::Effort,
            SettingsField::Vision,
            SettingsField::PriceInput,
            SettingsField::PriceOutput,
        ],
        Modal::ProviderKey => &[SettingsField::ApiKey],
        // BuiltinMcp's fields depend on the server, so the app passes them
        // through `builtin_mcp_fields` instead of this static table.
        _ => &[],
    }
}

/// The fields for configuring a built-in MCP server: coolify and dokploy take
/// a URL and a token; a VPS takes host, user and port, and any of a key file,
/// its passphrase and a password (with none, ssh-agent and `~/.ssh` keys).
pub fn builtin_mcp_fields(server: &str) -> &'static [SettingsField] {
    match server {
        "vps" => &[
            SettingsField::Name,
            SettingsField::Host,
            SettingsField::User,
            SettingsField::Port,
            SettingsField::KeyFile,
            SettingsField::Passphrase,
            SettingsField::Password,
        ],
        // A Postgres with pgvector, local or in the cloud, and a Voyage key.
        "rag" => &[SettingsField::Dsn, SettingsField::ApiKey],
        _ => &[SettingsField::BaseUrl, SettingsField::ApiKey],
    }
}

/// The fields of Settings > Team: with it off, only the switch; on, each
/// part and how review runs.
pub fn team_fields(enabled: bool) -> &'static [SettingsField] {
    if enabled {
        &[
            SettingsField::TeamEnabled,
            SettingsField::TeamMessages,
            SettingsField::TeamBoard,
            SettingsField::TeamReview,
            SettingsField::ReviewRounds,
            SettingsField::Reviewer,
        ]
    } else {
        &[SettingsField::TeamEnabled]
    }
}

/// The fields of Settings > RAG for a provider: a known provider's models,
/// widths and rerankers are picked; a custom endpoint's are typed.
pub fn rag_fields(provider: &str) -> &'static [SettingsField] {
    use enowx_core::builtin_mcp::rag::Provider;
    match Provider::parse(provider) {
        Provider::Voyage => &[
            SettingsField::RagEnabled,
            SettingsField::Dsn,
            SettingsField::RagProvider,
            SettingsField::ApiKey,
            SettingsField::EmbedModel,
            SettingsField::Dimension,
            SettingsField::Rerank,
            SettingsField::AutoIndex,
        ],
        Provider::OpenAi => &[
            SettingsField::RagEnabled,
            SettingsField::Dsn,
            SettingsField::RagProvider,
            SettingsField::ApiKey,
            SettingsField::EmbedModel,
            SettingsField::Dimension,
            SettingsField::AutoIndex,
        ],
        Provider::Custom => &[
            SettingsField::RagEnabled,
            SettingsField::Dsn,
            SettingsField::RagProvider,
            SettingsField::EmbedUrl,
            SettingsField::ApiKey,
            SettingsField::EmbedModelText,
            SettingsField::DimensionText,
            SettingsField::RerankText,
            SettingsField::AutoIndex,
        ],
    }
}

/// What the provider and model forms are editing.
#[derive(Clone, Default)]
pub struct SettingsDraft {
    /// The provider being edited or given a key; empty for a new one.
    pub provider_id: String,
    pub name: String,
    pub base_url: String,
    /// A key typed here. Empty keeps the one already stored.
    pub api_key: String,
    pub models_url: String,
    pub model: String,
    pub context_window: String,
    /// The chosen thinking effort, or empty for the provider default.
    pub effort: String,
    /// "yes" / "no", or empty to leave it to the catalogue.
    pub vision: String,
    pub price_input: String,
    pub price_output: String,
    /// A database connection string, for the RAG server.
    pub dsn: String,
    /// A VPS's key file and its passphrase.
    pub key_file: String,
    pub passphrase: String,
    /// Settings > RAG: "on" or "off", the provider's id, the vector width
    /// (empty for the model's default) and the reranker (empty for the
    /// provider's default, "off" for none). The model is in `model`.
    pub rag_enabled: String,
    /// "off" to stop indexing by itself; anything else is on.
    pub auto_index: String,
    pub rag_provider: String,
    pub dimension: String,
    pub rerank: String,
    /// Settings > Team: "on"/"off" switches, the rounds ("1" to "5"), the
    /// reviewer's id, and the agents that can review, to cycle through.
    pub team_enabled: String,
    pub team_messages: String,
    pub team_board: String,
    pub team_review: String,
    pub review_rounds: String,
    pub reviewer: String,
    pub reviewers: Vec<String>,
    /// Settings > Updates: "on" or "off".
    pub update_check: String,
    pub update_auto: String,
    /// The values of `CONF_FIELDS`, by index, as `Config::get` shows them.
    pub conf: Vec<String>,
    /// Settings > Decision model, by `DecField::key`.
    pub dec: std::collections::BTreeMap<&'static str, String>,
    /// An ACP agent's card or the custom agent form, by `AcpField::key`.
    pub acp: std::collections::BTreeMap<String, String>,
    /// The models and efforts the agent offers, as (value, name), to cycle
    /// through; empty until it has been asked.
    pub acp_models: Vec<(String, String)>,
    pub acp_efforts: Vec<(String, String)>,
    /// The efforts this model offers, to cycle through on the Effort field.
    pub efforts: Vec<String>,
}

impl SettingsDraft {
    /// The form for `connection`, its key left empty: a stored key is
    /// never shown, and typing one replaces it.
    pub fn for_connection(connection: &enowx_core::Connection) -> Self {
        Self {
            provider_id: connection.id.clone(),
            name: connection.name.clone(),
            base_url: connection.base_url.clone(),
            api_key: String::new(),
            models_url: connection.models_url.clone(),
            model: String::new(),
            context_window: String::new(),
            ..Self::default()
        }
    }

    pub fn value(&self, field: SettingsField) -> &str {
        match field {
            SettingsField::Name => &self.name,
            SettingsField::BaseUrl => &self.base_url,
            SettingsField::ApiKey => &self.api_key,
            SettingsField::ModelsUrl => &self.models_url,
            SettingsField::Model => &self.model,
            SettingsField::ContextWindow => &self.context_window,
            SettingsField::Effort => &self.effort,
            SettingsField::Vision => &self.vision,
            SettingsField::PriceInput => &self.price_input,
            SettingsField::PriceOutput => &self.price_output,
            SettingsField::Host => &self.base_url,
            SettingsField::User => &self.models_url,
            SettingsField::Port => &self.context_window,
            SettingsField::Dsn => &self.dsn,
            SettingsField::KeyFile => &self.key_file,
            SettingsField::Passphrase => &self.passphrase,
            SettingsField::Password => &self.api_key,
            SettingsField::RagEnabled => &self.rag_enabled,
            SettingsField::RagProvider => &self.rag_provider,
            SettingsField::EmbedModel | SettingsField::EmbedModelText => &self.model,
            SettingsField::Dimension | SettingsField::DimensionText => &self.dimension,
            SettingsField::Rerank | SettingsField::RerankText => &self.rerank,
            SettingsField::EmbedUrl => &self.base_url,
            SettingsField::AutoIndex => &self.auto_index,
            SettingsField::TeamEnabled => &self.team_enabled,
            SettingsField::TeamMessages => &self.team_messages,
            SettingsField::TeamBoard => &self.team_board,
            SettingsField::TeamReview => &self.team_review,
            SettingsField::ReviewRounds => &self.review_rounds,
            SettingsField::Reviewer => &self.reviewer,
            SettingsField::UpdateCheck => &self.update_check,
            SettingsField::UpdateAuto => &self.update_auto,
            SettingsField::Conf(i) => self.conf.get(i).map_or("", String::as_str),
            SettingsField::Dec(field) => self.dec.get(field.key()).map_or("", String::as_str),
            SettingsField::Acp(field) => self.acp.get(&field.key()).map_or("", String::as_str),
        }
    }

    pub fn value_mut(&mut self, field: SettingsField) -> &mut String {
        match field {
            SettingsField::Name => &mut self.name,
            SettingsField::BaseUrl => &mut self.base_url,
            SettingsField::ApiKey => &mut self.api_key,
            SettingsField::ModelsUrl => &mut self.models_url,
            SettingsField::Model => &mut self.model,
            SettingsField::ContextWindow => &mut self.context_window,
            SettingsField::Effort => &mut self.effort,
            SettingsField::Vision => &mut self.vision,
            SettingsField::PriceInput => &mut self.price_input,
            SettingsField::PriceOutput => &mut self.price_output,
            SettingsField::Host => &mut self.base_url,
            SettingsField::User => &mut self.models_url,
            SettingsField::Port => &mut self.context_window,
            SettingsField::Dsn => &mut self.dsn,
            SettingsField::KeyFile => &mut self.key_file,
            SettingsField::Passphrase => &mut self.passphrase,
            SettingsField::Password => &mut self.api_key,
            SettingsField::RagEnabled => &mut self.rag_enabled,
            SettingsField::RagProvider => &mut self.rag_provider,
            SettingsField::EmbedModel | SettingsField::EmbedModelText => &mut self.model,
            SettingsField::Dimension | SettingsField::DimensionText => &mut self.dimension,
            SettingsField::Rerank | SettingsField::RerankText => &mut self.rerank,
            SettingsField::EmbedUrl => &mut self.base_url,
            SettingsField::AutoIndex => &mut self.auto_index,
            SettingsField::TeamEnabled => &mut self.team_enabled,
            SettingsField::TeamMessages => &mut self.team_messages,
            SettingsField::TeamBoard => &mut self.team_board,
            SettingsField::TeamReview => &mut self.team_review,
            SettingsField::ReviewRounds => &mut self.review_rounds,
            SettingsField::Reviewer => &mut self.reviewer,
            SettingsField::UpdateCheck => &mut self.update_check,
            SettingsField::UpdateAuto => &mut self.update_auto,
            SettingsField::Conf(i) => {
                if self.conf.len() <= i {
                    self.conf.resize(i + 1, String::new());
                }
                &mut self.conf[i]
            }
            SettingsField::Dec(field) => self.dec.entry(field.key()).or_default(),
            SettingsField::Acp(field) => self.acp.entry(field.key()).or_default(),
        }
    }

    /// The RAG setup this draft describes.
    pub fn rag_setup(&self) -> enowx_core::builtin_mcp::rag::RagSetup {
        use enowx_core::builtin_mcp::rag::{Provider, RagSetup};
        let provider = Provider::parse(&self.rag_provider);
        RagSetup {
            provider: provider.id().to_owned(),
            base_url: if provider == Provider::Custom {
                self.base_url.trim().to_owned()
            } else {
                String::new()
            },
            model: self.model.trim().to_owned(),
            dimension: self.dimension.trim().parse().unwrap_or(0),
            rerank: self.rerank.trim().to_owned(),
            // Saved only when off: on is the default.
            auto_index: (self.auto_index == "off").then_some(false),
        }
    }

    /// What a cycled choice field shows, and how to step it. `Effort` cycles
    /// through the model's efforts plus a leading "default"; `Vision` toggles
    /// default / yes / no.
    pub fn choice_shown(&self, field: SettingsField) -> String {
        match field {
            SettingsField::Effort => {
                if self.effort.is_empty() {
                    "default".into()
                } else {
                    self.effort.clone()
                }
            }
            SettingsField::Vision => match self.vision.as_str() {
                "yes" => "yes".into(),
                "no" => "no".into(),
                _ => "default".into(),
            },
            SettingsField::RagEnabled => {
                if self.rag_enabled == "on" {
                    "on".into()
                } else {
                    "off".into()
                }
            }
            SettingsField::AutoIndex => {
                if self.auto_index == "off" {
                    "off".into()
                } else {
                    "on".into()
                }
            }
            SettingsField::TeamEnabled
            | SettingsField::TeamMessages
            | SettingsField::TeamBoard
            | SettingsField::TeamReview
            | SettingsField::UpdateCheck
            | SettingsField::UpdateAuto => {
                if self.value(field) == "on" {
                    "on".into()
                } else {
                    "off".into()
                }
            }
            SettingsField::ReviewRounds => {
                if self.review_rounds.is_empty() {
                    "2".into()
                } else {
                    self.review_rounds.clone()
                }
            }
            SettingsField::Reviewer => enowx_core::agent_def::display_name(&self.reviewer),
            SettingsField::Conf(i) => {
                let value = self.value(field);
                match CONF_FIELDS.get(i).map(|f| f.2) {
                    Some(ConfKind::Bool) => if value == "true" { "on" } else { "off" }.into(),
                    // A fraction of the context, read as a percentage.
                    Some(ConfKind::Choice(_)) if CONF_FIELDS[i].0 == "agent.auto_compact_at" => {
                        value.parse::<f32>().map_or_else(
                            |_| value.to_owned(),
                            |f| format!("{}%", (f * 100.0).round()),
                        )
                    }
                    _ => value.to_owned(),
                }
            }
            SettingsField::RagProvider => {
                enowx_core::builtin_mcp::rag::Provider::parse(&self.rag_provider)
                    .label()
                    .into()
            }
            SettingsField::EmbedModel => self.rag_setup().model(),
            SettingsField::Dimension => self.rag_setup().dimension().to_string(),
            SettingsField::Rerank => self.rag_setup().reranker().unwrap_or_else(|| "off".into()),
            SettingsField::Dec(dec) => {
                let value = self.value(field);
                match dec {
                    DecField::Enabled | DecField::Shadow | DecField::UseOn(_) => {
                        if value == "on" { "on" } else { "off" }.into()
                    }
                    DecField::Provider => enowx_core::decision::provider_label(value).into(),
                    DecField::ModelPick if value.is_empty() => {
                        let provider = self.value(SettingsField::Dec(DecField::Provider));
                        enowx_core::decision::models(provider)
                            .first()
                            .copied()
                            .unwrap_or_default()
                            .into()
                    }
                    DecField::Timeout => format!("{value} ms"),
                    _ => value.to_owned(),
                }
            }
            SettingsField::Acp(acp) => {
                let value = self.value(field);
                let name_in = |list: &[(String, String)]| {
                    list.iter()
                        .find(|(v, _)| v == value)
                        .map_or_else(|| value.to_owned(), |(_, n)| n.clone())
                };
                match acp {
                    AcpField::Model if value.is_empty() && self.acp_models.is_empty() => {
                        "its default (Enter on Status lists its models)".into()
                    }
                    AcpField::Model | AcpField::Effort if value.is_empty() => "its default".into(),
                    AcpField::Model => name_in(&self.acp_models),
                    AcpField::Effort => name_in(&self.acp_efforts),
                    AcpField::Permission | AcpField::CustomPermission => match value {
                        "enowx" => "enowx's rules (like enowx's own tools)".into(),
                        "bypass" => "bypass: it asks nothing (careful)".into(),
                        _ => "ask me".into(),
                    },
                    _ => value.to_owned(),
                }
            }
            _ => self.value(field).to_owned(),
        }
    }

    /// Step a cycled choice by `delta` (+1 / -1).
    pub fn cycle_choice(&mut self, field: SettingsField, delta: i32) {
        match field {
            SettingsField::Effort => {
                let mut options = vec![String::new()];
                options.extend(self.efforts.iter().cloned());
                let here = options.iter().position(|o| *o == self.effort).unwrap_or(0);
                let len = options.len() as i32;
                let next = (((here as i32 + delta) % len) + len) % len;
                self.effort = options[next as usize].clone();
            }
            SettingsField::RagEnabled => {
                self.rag_enabled = if self.rag_enabled == "on" {
                    "off"
                } else {
                    "on"
                }
                .into();
            }
            SettingsField::AutoIndex => {
                self.auto_index = if self.auto_index == "off" {
                    "on"
                } else {
                    "off"
                }
                .into();
            }
            SettingsField::TeamEnabled
            | SettingsField::TeamMessages
            | SettingsField::TeamBoard
            | SettingsField::TeamReview
            | SettingsField::UpdateCheck
            | SettingsField::UpdateAuto => {
                let next = if self.value(field) == "on" {
                    "off"
                } else {
                    "on"
                };
                *self.value_mut(field) = next.into();
            }
            SettingsField::ReviewRounds => {
                let here: i32 = self.review_rounds.parse().unwrap_or(2);
                let next = ((here - 1 + delta).rem_euclid(5)) + 1;
                self.review_rounds = next.to_string();
            }
            SettingsField::Dec(dec) => {
                let current = self.value(field).to_owned();
                let step = |options: &[&str]| -> String {
                    let here = options.iter().position(|o| *o == current).unwrap_or(0) as i32;
                    let len = options.len().max(1) as i32;
                    options
                        .get((here + delta).rem_euclid(len) as usize)
                        .map(|o| (*o).to_owned())
                        .unwrap_or_default()
                };
                let next = match dec {
                    DecField::Enabled | DecField::Shadow | DecField::UseOn(_) => {
                        if current == "on" { "off" } else { "on" }.to_owned()
                    }
                    DecField::Provider => step(&enowx_core::decision::PROVIDERS),
                    DecField::ModelPick => {
                        let provider = self
                            .value(SettingsField::Dec(DecField::Provider))
                            .to_owned();
                        step(enowx_core::decision::models(&provider))
                    }
                    DecField::Timeout => step(&DECISION_TIMEOUTS),
                    DecField::Threshold(_) => step(&DECISION_THRESHOLDS),
                    _ => return,
                };
                if dec == DecField::Provider && next != current {
                    // Another provider's model means nothing to this one.
                    self.dec.insert("model", String::new());
                }
                *self.value_mut(field) = next;
            }
            SettingsField::Acp(acp) => {
                let current = self.value(field).to_owned();
                let listed = |list: &[(String, String)]| -> Vec<String> {
                    std::iter::once(String::new())
                        .chain(list.iter().map(|(v, _)| v.clone()))
                        .collect()
                };
                let owned: Vec<String> = match acp {
                    AcpField::Model => listed(&self.acp_models),
                    AcpField::Effort if !self.acp_efforts.is_empty() => listed(&self.acp_efforts),
                    AcpField::Effort => ACP_EFFORTS.iter().map(|s| (*s).to_owned()).collect(),
                    AcpField::Permission | AcpField::CustomPermission => {
                        enowx_core::acp::PERMISSIONS
                            .iter()
                            .map(|s| (*s).to_owned())
                            .collect()
                    }
                    _ => return,
                };
                let options: Vec<&str> = owned.iter().map(String::as_str).collect();
                let here = options.iter().position(|o| *o == current).unwrap_or(0) as i32;
                let next = options[(here + delta).rem_euclid(options.len() as i32) as usize];
                *self.value_mut(field) = next.to_owned();
            }
            SettingsField::Conf(i) => match CONF_FIELDS.get(i).map(|f| f.2) {
                Some(ConfKind::Bool) => {
                    let next = if self.value(field) == "true" {
                        "false"
                    } else {
                        "true"
                    };
                    *self.value_mut(field) = next.into();
                }
                Some(ConfKind::Choice(options)) => {
                    let current = self.value(field).to_owned();
                    // `0.85` and `0.850` are one value.
                    let here = options
                        .iter()
                        .position(|o| {
                            *o == current
                                || o.parse::<f64>()
                                    .ok()
                                    .zip(current.parse::<f64>().ok())
                                    .is_some_and(|(a, b)| (a - b).abs() < 1e-6)
                        })
                        .unwrap_or(0) as i32;
                    let len = options.len() as i32;
                    let next = (here + delta).rem_euclid(len);
                    *self.value_mut(field) = options[next as usize].into();
                }
                _ => {}
            },
            SettingsField::Reviewer => {
                if self.reviewers.is_empty() {
                    return;
                }
                let here = self
                    .reviewers
                    .iter()
                    .position(|r| *r == self.reviewer)
                    .unwrap_or(0) as i32;
                let len = self.reviewers.len() as i32;
                let next = (here + delta).rem_euclid(len);
                self.reviewer = self.reviewers[next as usize].clone();
            }
            // A new provider starts on its own defaults: another provider's
            // model, width or reranker means nothing there.
            SettingsField::RagProvider => {
                use enowx_core::builtin_mcp::rag::Provider;
                let all = Provider::ALL;
                let here = all
                    .iter()
                    .position(|p| *p == Provider::parse(&self.rag_provider))
                    .unwrap_or(0) as i32;
                let len = all.len() as i32;
                let next = (((here + delta) % len) + len) % len;
                self.rag_provider = all[next as usize].id().into();
                self.model.clear();
                self.dimension.clear();
                self.rerank.clear();
                self.base_url.clear();
            }
            SettingsField::EmbedModel => {
                let setup = self.rag_setup();
                let models: Vec<&str> = setup.provider().models().iter().map(|(m, _)| *m).collect();
                if models.is_empty() {
                    return;
                }
                let here = models.iter().position(|m| *m == setup.model()).unwrap_or(0) as i32;
                let len = models.len() as i32;
                let next = (((here + delta) % len) + len) % len;
                self.model = models[next as usize].into();
                self.dimension.clear();
            }
            SettingsField::Dimension => {
                let setup = self.rag_setup();
                let model = setup.model();
                let Some((_, dims)) = setup.provider().models().iter().find(|(m, _)| *m == model)
                else {
                    return;
                };
                let mut dims = dims.to_vec();
                dims.sort_unstable();
                let here = dims
                    .iter()
                    .position(|d| *d == setup.dimension())
                    .unwrap_or(0) as i32;
                let len = dims.len() as i32;
                let next = (((here + delta) % len) + len) % len;
                self.dimension = dims[next as usize].to_string();
            }
            SettingsField::Rerank => {
                let setup = self.rag_setup();
                let mut options: Vec<String> = setup
                    .provider()
                    .rerankers()
                    .iter()
                    .map(|r| (*r).to_owned())
                    .collect();
                options.push("off".into());
                let current = setup.reranker().unwrap_or_else(|| "off".into());
                let here = options.iter().position(|o| *o == current).unwrap_or(0) as i32;
                let len = options.len() as i32;
                let next = (((here + delta) % len) + len) % len;
                self.rerank = options[next as usize].clone();
            }
            SettingsField::Vision => {
                let options = ["", "yes", "no"];
                let here = options.iter().position(|o| *o == self.vision).unwrap_or(0);
                let len = options.len() as i32;
                let next = (((here as i32 + delta) % len) + len) % len;
                self.vision = options[next as usize].to_owned();
            }
            _ => {}
        }
    }
}

/// Fields in the `Add MCP` popup, tabbed through with Up/Down.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum McpFormField {
    Name,
    Command,
    Args,
    Env,
    Transport,
}

pub const MCP_FORM_FIELDS: [McpFormField; 5] = [
    McpFormField::Name,
    McpFormField::Command,
    McpFormField::Args,
    McpFormField::Env,
    McpFormField::Transport,
];

pub const MCP_FORM_LABELS: [&str; 5] = [
    "Name",
    "Command / URL",
    "Args (comma-separated)",
    "Env (KEY=VAL per line)",
    "Transport (stdio/http/sse)",
];

impl McpFormField {
    pub fn placeholder(self) -> &'static str {
        match self {
            McpFormField::Name => "e.g. github",
            McpFormField::Command => "e.g. npx or https://…",
            McpFormField::Args => "-y, @modelcontextprotocol/server-github",
            McpFormField::Env => "GITHUB_TOKEN=...",
            McpFormField::Transport => "stdio",
        }
    }
}
