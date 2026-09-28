//! Modal pickers and the provider/model settings draft they edit.

pub use enowx_core::persist::McpDraft;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    /// The agent roster as a picker.
    Agents,
    /// Actions on a sent message: edit, resend, copy.
    Message,
    /// TypeSafe's key and toggles. Separate from the provider list because
    /// nothing here can answer a prompt — offering it as a chat provider
    /// would be offering something that does not exist.
    TypeSafe,
    /// Entering the TypeSafe key. A form so the value is masked as it is
    /// typed, the same as the provider key.
    TypeSafeKey,
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
    Themes,
    Attach,
    /// Floating list of every command, searchable. Distinct from the
    /// inline list that appears above the composer when the input starts
    /// with `/`: that one is for people who know the name they want, this is
    /// for looking.
    Commands,
    Skills,
    Mcp,
    McpForm,
    /// Ctrl+C in an empty composer: confirm before quitting.
    QuitConfirm,
}

impl Modal {
    pub fn title(self) -> &'static str {
        match self {
            Modal::Commands => " COMMANDS ",
            Modal::Agents => " AGENT ",
            Modal::Message => " MESSAGE ",
            Modal::TypeSafe => " TYPESAFE ",
            Modal::TypeSafeKey => "",
            Modal::MessageEdit => " EDIT PROMPT ",
            Modal::Sessions => " RESUME SESSION ",
            Modal::Providers => " PROVIDERS ",
            Modal::Models => " MODELS ",
            Modal::Themes => " THEME ",
            Modal::Attach => " ATTACH IMAGE ",
            Modal::Skills => " SKILLS ",
            Modal::Mcp => " MCP SERVERS ",
            Modal::McpForm => " ADD MCP SERVER ",
            Modal::QuitConfirm => " QUIT ENX ",
            // Forms draw their own heading, so the generic title is empty.
            Modal::None | Modal::ProviderForm | Modal::ModelManual | Modal::ProviderKey => "",
        }
    }

    /// Text-field modals share one key handler and one renderer.
    pub fn is_form(self) -> bool {
        matches!(
            self,
            Modal::ProviderForm
                | Modal::ModelManual
                | Modal::ProviderKey
                | Modal::TypeSafeKey
                | Modal::McpForm
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
}

impl SettingsField {
    pub fn label(self) -> &'static str {
        match self {
            SettingsField::Name => "Name",
            SettingsField::BaseUrl => "Base URL",
            SettingsField::ApiKey => "API key",
            SettingsField::ModelsUrl => "Model-list URL",
            SettingsField::Model => "Model ID",
            SettingsField::ContextWindow => "Context window (tokens)",
        }
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
        Modal::ProviderKey | Modal::TypeSafeKey => &[SettingsField::ApiKey],
        _ => &[],
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
