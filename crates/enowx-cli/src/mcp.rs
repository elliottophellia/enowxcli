//! `enx mcp` and `enx vps`: install the MCP servers built into enx, and the
//! VPSes the `vps` server reaches.

use std::io::{BufRead as _, IsTerminal as _, Write as _};

use anyhow::{bail, Context as _, Result};
use enowx_core::{
    auth::Auth,
    builtin_mcp::{self, secret_id, vps, BuiltinConfig, Endpoint},
};

use crate::auth::read_key;

/// A visible answer, typed or piped.
fn read_line(prompt: &str) -> Result<String> {
    if std::io::stdin().is_terminal() {
        eprint!("{prompt}");
        std::io::stderr().flush()?;
    }
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

/// `enx mcp list`: each built-in server and whether it is installed.
pub fn list() -> Result<()> {
    let config = BuiltinConfig::load()?;
    for name in builtin_mcp::NAMES {
        let detail = match name {
            "coolify" => config.coolify.as_ref().map(|e| e.base_url.clone()),
            "dokploy" => config.dokploy.as_ref().map(|e| e.base_url.clone()),
            "rag" => config
                .rag
                .as_ref()
                .map(|_| "Voyage AI + pgvector".to_owned()),
            _ if config.vps.is_empty() => None,
            _ => Some(format!(
                "{} VPS: {}",
                config.vps.len(),
                config.vps.keys().cloned().collect::<Vec<_>>().join(", ")
            )),
        };
        let on = enowx_core::persist::read_overrides()
            .get(name)
            .and_then(|o| o.enabled)
            .unwrap_or(false);
        match detail {
            Some(detail) => println!(
                "{name:8} {:<13} {detail}",
                if on {
                    "configured, on"
                } else {
                    "configured, off"
                }
            ),
            None => println!("{name:8} {:<13} (enx {})", "not set up", install_hint(name)),
        }
    }
    Ok(())
}

fn install_hint(name: &str) -> &'static str {
    match name {
        "vps" => "vps add <name> --host <address> --user <user>",
        "coolify" => "mcp set coolify --url <url> --token <token>",
        "rag" => "mcp set rag --dsn <postgres://...> --token <voyage key>",
        _ => "mcp set dokploy --url <url> --token <token>",
    }
}

/// `enx mcp install coolify|dokploy`: the panel's URL and an API token.
pub fn install(name: &str, url: Option<String>, token: Option<String>) -> Result<()> {
    let (label, token_hint) = match name {
        "coolify" => (
            "Coolify",
            "Create one in Coolify under Keys & Tokens > API tokens.",
        ),
        "dokploy" => (
            "Dokploy",
            "Create one in Dokploy under Settings > Profile > API/CLI.",
        ),
        "vps" => bail!(
            "VPSes are added one at a time: enx vps add <name> --host <address> --user <user>"
        ),
        other => bail!(
            "enx has no built-in MCP server `{other}` (built in: {})",
            builtin_mcp::NAMES.join(", ")
        ),
    };
    let url = match url {
        Some(url) => url,
        None => read_line(&format!("{label} URL (https://...): "))?,
    };
    let url = url.trim().trim_end_matches('/').to_owned();
    anyhow::ensure!(
        url.starts_with("https://") || url.starts_with("http://"),
        "the {label} URL must start with https:// or http://"
    );
    let token = match token {
        Some(token) => token,
        None => {
            eprintln!("{token_hint}");
            read_key(&format!("{label} API token: "))?
        }
    };
    anyhow::ensure!(!token.trim().is_empty(), "no token entered; nothing saved");

    let mut config = BuiltinConfig::load()?;
    let endpoint = Some(Endpoint {
        base_url: url.clone(),
    });
    match name {
        "coolify" => config.coolify = endpoint,
        _ => config.dokploy = endpoint,
    }
    let mut auth = Auth::load()?;
    auth.store(&secret_id(name), &token)?;
    let path = config.save()?;
    // Turn it on: a built-in server is off until configured, and filling it
    // in from the CLI is how the user (or an agent helping them) sets it up.
    enowx_core::persist::set_mcp_enabled(name, true)?;
    println!(
        "Configured {name} ({url}) and turned it on. Setup in {}",
        path.display()
    );
    Ok(())
}

/// `enx mcp set rag`: the database (a Postgres with pgvector, local or
/// cloud) and the Voyage AI key. Both are secrets and go to auth.json.
pub fn set_rag(dsn: Option<String>, token: Option<String>) -> Result<()> {
    use enowx_core::builtin_mcp::{rag, rag_dsn_id};
    let dsn = match dsn {
        Some(dsn) => dsn,
        None => read_key("Postgres connection string (postgres://...): ")?,
    };
    let dsn = dsn.trim().to_owned();
    anyhow::ensure!(
        dsn.starts_with("postgres://") || dsn.starts_with("postgresql://"),
        "the database must be a postgres:// connection string"
    );
    let token = match token {
        Some(token) => token,
        None => {
            eprintln!("Create a key at https://dash.voyageai.com/api-keys");
            read_key("Voyage AI API key: ")?
        }
    };
    anyhow::ensure!(!token.trim().is_empty(), "no key entered; nothing saved");
    let mut auth = Auth::load()?;
    auth.store(&rag_dsn_id(), &dsn)?;
    auth.store(&secret_id("rag"), token.trim())?;
    let mut config = BuiltinConfig::load()?;
    config.rag = Some(config.rag.take().unwrap_or_else(rag::RagSetup::default));
    config.save()?;
    enowx_core::persist::set_mcp_enabled("rag", true)?;
    println!("Configured rag and turned it on. Its search skill is offered to agents now.");
    Ok(())
}

/// `enx mcp uninstall <name>`: forget its setup and secrets.
pub fn uninstall(name: &str) -> Result<()> {
    let mut config = BuiltinConfig::load()?;
    let mut auth = Auth::load()?;
    match name {
        "coolify" => config.coolify = None,
        "dokploy" => config.dokploy = None,
        "rag" => {
            config.rag = None;
            auth.forget(&enowx_core::builtin_mcp::rag_dsn_id())?;
        }
        "vps" => {
            for host in config.vps.keys() {
                auth.forget(&vps::password_id(host))?;
            }
            config.vps.clear();
        }
        other => bail!("enx has no built-in MCP server `{other}`"),
    }
    auth.forget(&secret_id(name))?;
    config.save()?;
    enowx_core::persist::set_mcp_enabled(name, false)?;
    println!("Cleared {name} and turned it off");
    Ok(())
}

/// `enx vps add`: a VPS signing in with a key file, or a password typed now.
pub fn vps_add(
    name: &str,
    host: &str,
    user: &str,
    port: u16,
    key: Option<String>,
    password: Option<String>,
) -> Result<()> {
    anyhow::ensure!(
        vps::valid_name(name),
        "a VPS name is letters, digits, - and _ (at most 40)"
    );
    anyhow::ensure!(!host.trim().is_empty(), "--host is required");
    anyhow::ensure!(!user.trim().is_empty(), "--user is required");
    let mut auth = Auth::load()?;
    let key_path = match key {
        Some(path) => {
            let expanded = path.replacen('~', &std::env::var("HOME").unwrap_or_default(), 1);
            anyhow::ensure!(
                std::path::Path::new(&expanded).is_file(),
                "no key file at {path}"
            );
            auth.forget(&vps::password_id(name))?;
            Some(path)
        }
        None => {
            let password = match password {
                Some(password) => password,
                None => read_key(&format!("Password for {user}@{host}: "))?,
            };
            anyhow::ensure!(!password.is_empty(), "no password entered; nothing saved");
            auth.store(&vps::password_id(name), &password)
                .context("storing the password")?;
            None
        }
    };
    let mut config = BuiltinConfig::load()?;
    let replaced = config
        .vps
        .insert(
            name.to_owned(),
            vps::Host {
                host: host.trim().to_owned(),
                port,
                user: user.trim().to_owned(),
                key_path,
            },
        )
        .is_some();
    config.save()?;
    // A configured VPS turns the vps server on.
    enowx_core::persist::set_mcp_enabled("vps", true)?;
    println!(
        "{} VPS {name} ({user}@{host}:{port}) and turned the vps server on. Its host key is recorded on the first connection.",
        if replaced { "Updated" } else { "Added" }
    );
    Ok(())
}

pub fn vps_list() -> Result<()> {
    let config = BuiltinConfig::load()?;
    if config.vps.is_empty() {
        println!("No VPS set up. Add one: enx vps add <name> --host <address> --user <user> [--key <file>]");
    }
    for (name, host) in &config.vps {
        println!(
            "{name:16} {}@{}:{}   {}",
            host.user,
            host.host,
            host.port,
            host.key_path
                .as_deref()
                .map(|k| format!("key {k}"))
                .unwrap_or_else(|| "password".into())
        );
    }
    Ok(())
}

pub fn vps_remove(name: &str) -> Result<()> {
    let mut config = BuiltinConfig::load()?;
    if config.vps.remove(name).is_none() {
        bail!("no VPS named {name}");
    }
    Auth::load()?.forget(&vps::password_id(name))?;
    config.save()?;
    println!("Removed VPS {name}");
    Ok(())
}
