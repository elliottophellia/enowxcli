use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use enowx_core::Config;

mod auth;
mod dev;
mod mcp;

#[derive(Debug, Parser)]
#[command(
    name = "enx",
    version,
    about = "Rust coding agent with the enowxcli terminal interface"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open the enowxcli terminal interface.
    Tui {
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Resume this session id instead of starting a new conversation.
        #[arg(long)]
        session: Option<String>,
    },
    /// Rebuild and relaunch the interface whenever a source file changes.
    Dev {
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Pin one session across restarts; defaults to the newest one.
        #[arg(long)]
        session: Option<String>,
    },
    /// Read or change ~/.enx/config.toml.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// The MCP servers built into enx (coolify, dokploy, vps): install them
    /// to offer them to agents.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// The VPSes the built-in `vps` MCP server reaches over SSH.
    Vps {
        #[command(subcommand)]
        command: VpsCommand,
    },
    /// Provider keys, kept in ~/.enx/auth.json.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// List the providers enx knows and which of them are connected.
    #[command(alias = "ls")]
    List,
    /// Store a provider's API key, typed at a prompt or piped in.
    Login { provider: String },
    /// Remove a provider's stored API key.
    Logout { provider: String },
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    /// Each built-in server and whether it is installed.
    #[command(alias = "ls")]
    List,
    /// Set up coolify or dokploy: its URL, then an API token at a prompt.
    Install {
        name: String,
        #[arg(long)]
        url: Option<String>,
    },
    /// Remove a built-in server's setup and stored secrets.
    Uninstall { name: String },
    /// Run a built-in server over stdio, as an MCP client starts it.
    #[command(hide = true)]
    Serve { name: String },
}

#[derive(Debug, Subcommand)]
enum VpsCommand {
    /// Add or update a VPS. Without --key, its password is asked for and
    /// kept in auth.json.
    Add {
        name: String,
        #[arg(long)]
        host: String,
        #[arg(long)]
        user: String,
        #[arg(long, default_value_t = 22)]
        port: u16,
        /// A private key file to sign in with instead of a password.
        #[arg(long)]
        key: Option<String>,
    },
    #[command(alias = "ls")]
    List,
    #[command(alias = "rm")]
    Remove { name: String },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print a dotted key, for example `model.active` or `provider.enowx.base_url`.
    Get { key: String },
    /// Set and persist a dotted key: `model.default deepseek/deepseek-flash`
    /// pins the model to start on, `provider.<id>.base_url <url>` adds a
    /// provider.
    Set { key: String, value: String },
    /// Print the config file path.
    Path,
}

#[tokio::main]
async fn main() -> Result<()> {
    // The alternate-screen TUI owns stdout, so nothing here logs to it.
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Tui {
        workspace: None,
        session: None,
    }) {
        Command::Tui { workspace, session } => {
            let mut config = Config::load()?;
            if workspace.is_some() {
                config.agent.workspace = workspace;
            }
            enowx_tui::run(config, session).await
        }
        Command::Dev { workspace, session } => dev::run(workspace, session),
        Command::Config { command } => {
            let mut config = Config::load()?;
            match command {
                ConfigCommand::Get { key } => {
                    let value = config
                        .get(&key)
                        .ok_or_else(|| anyhow::anyhow!("unknown config key: {key}"))?;
                    println!("{value}");
                }
                ConfigCommand::Set { key, value } => {
                    config.set(&key, &value)?;
                    let path = config.save()?;
                    println!("Updated {key}");
                    println!("saved {}", path.display());
                }
                ConfigCommand::Path => println!("{}", enowx_core::config::config_path().display()),
            }
            Ok(())
        }
        Command::Mcp { command } => match command {
            McpCommand::List => mcp::list(),
            McpCommand::Install { name, url } => mcp::install(&name, url),
            McpCommand::Uninstall { name } => mcp::uninstall(&name),
            McpCommand::Serve { name } => enowx_core::builtin_mcp::serve(&name).await,
        },
        Command::Vps { command } => match command {
            VpsCommand::Add {
                name,
                host,
                user,
                port,
                key,
            } => mcp::vps_add(&name, &host, &user, port, key),
            VpsCommand::List => mcp::vps_list(),
            VpsCommand::Remove { name } => mcp::vps_remove(&name),
        },
        Command::Auth { command } => {
            let mut config = Config::load()?;
            match command {
                AuthCommand::List => auth::list(&config),
                AuthCommand::Login { provider } => auth::login(&mut config, &provider)?,
                AuthCommand::Logout { provider } => auth::logout(&mut config, &provider)?,
            }
            Ok(())
        }
    }
}
