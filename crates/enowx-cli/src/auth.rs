//! `enx auth`: provider keys in `~/.enx/auth.json`. A key is typed at a prompt that does not echo it, or piped
//! in, so it never lands in shell history.

use std::io::{BufRead as _, IsTerminal as _, Write as _};

use anyhow::{Context as _, Result};
use enowx_core::{auth::KeySource, Config};

/// Every provider enx knows, and whether it has a key and from where.
pub fn list(config: &Config) {
    for connection in config.connections() {
        let state = match &connection.key_source {
            Some(KeySource::Stored) => "connected, key in auth.json".to_owned(),
            Some(KeySource::Env(name)) => format!("connected, key from {name}"),
            Some(KeySource::Session) => "connected for this process".to_owned(),
            None if connection.is_local() => "local, needs no key".to_owned(),
            None => "not connected".to_owned(),
        };
        println!("{:<14} {:<22} {state}", connection.id, connection.name);
    }
    println!();
    println!("keys: {}", enowx_core::auth::auth_path().display());
}

/// Where to create a Cloudflare token, and the scope levels to pick from. Only
/// Zone:Read is needed to verify control of a domain for an assessment.
const CLOUDFLARE_TOKEN_HELP: &str = "\
Create a token at https://dash.cloudflare.com/profile/api-tokens with one of:\n\
  - minimal (recommended): Zone / Zone / Read, for the zones you want to test. \
Enough to prove control of a domain.\n\
  - medium: add Zone / DNS / Read, so records resolve without a separate lookup.\n\
  - full: an account-wide token. More than an assessment needs; avoid unless you \
have a reason.\n\
Scope the token to the specific zones, not all of them, when you can.";

/// Ask for `provider`'s key and store it. `cloudflare` is not a model provider
/// but a connected account enx uses to verify control of a domain before a
/// security assessment, so it is stored the same way under its own id.
pub fn login(config: &mut Config, provider: &str) -> Result<()> {
    if provider.eq_ignore_ascii_case("cloudflare") {
        eprintln!("{CLOUDFLARE_TOKEN_HELP}");
        let key = read_key("Cloudflare API token: ")?;
        anyhow::ensure!(!key.trim().is_empty(), "no token entered; nothing saved");
        config.auth.store("cloudflare", &key)?;
        println!(
            "Saved the Cloudflare token to {}. The security team can now verify control of a \
             domain before testing it.",
            enowx_core::auth::auth_path().display()
        );
        return Ok(());
    }
    let connection = config.connection(provider).ok_or_else(|| {
        let built_in: Vec<&str> = enowx_core::PROVIDER_PRESETS.iter().map(|p| p.id).collect();
        anyhow::anyhow!(
            "enx knows no provider `{provider}`. Built in: {}. Add your own with \
             `enx config set provider.{provider}.base_url <url>` first.",
            built_in.join(", ")
        )
    })?;
    if let Some(url) = connection
        .preset
        .map(|p| p.key_url)
        .filter(|u| !u.is_empty())
    {
        eprintln!("Create a key at {url}");
    }
    let key = read_key(&format!("{} API key: ", connection.name))?;
    anyhow::ensure!(!key.trim().is_empty(), "no key entered; nothing saved");
    config.auth.store(&connection.id, &key)?;
    println!(
        "Saved the {} key to {}",
        connection.name,
        enowx_core::auth::auth_path().display()
    );
    Ok(())
}

/// Remove `provider`'s stored key.
pub fn logout(config: &mut Config, provider: &str) -> Result<()> {
    if provider.eq_ignore_ascii_case("cloudflare") {
        if config.auth.forget("cloudflare")? {
            println!("Removed the Cloudflare token from auth.json");
        } else {
            println!("auth.json holds no Cloudflare token");
        }
        return Ok(());
    }
    if config.auth.forget(provider)? {
        println!("Removed the {provider} key from auth.json");
    } else {
        println!("auth.json holds no key for {provider}");
    }
    // A key from the environment is still read; say so rather than let the
    // provider look disconnected when it is not.
    if let Some(Some(KeySource::Env(name))) = config.connection(provider).map(|c| c.key_source) {
        println!("{name} is still set, so {provider} stays connected");
    }
    Ok(())
}

/// A line from stdin when it is piped, or typed at a prompt with no echo.
fn read_key(prompt: &str) -> Result<String> {
    if !std::io::stdin().is_terminal() {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        return Ok(line.trim().to_owned());
    }
    use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    crossterm::terminal::enable_raw_mode().context("reading the key")?;
    let typed = (|| -> Result<Option<String>> {
        let mut key = String::new();
        loop {
            let Event::Key(event) = read()? else {
                continue;
            };
            if event.kind == KeyEventKind::Release {
                continue;
            }
            match event.code {
                KeyCode::Enter => return Ok(Some(key)),
                KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None)
                }
                KeyCode::Backspace => {
                    key.pop();
                }
                KeyCode::Char(c) => key.push(c),
                _ => {}
            }
        }
    })();
    crossterm::terminal::disable_raw_mode()?;
    eprintln!();
    typed?
        .map(|key| key.trim().to_owned())
        .ok_or_else(|| anyhow::anyhow!("cancelled; nothing saved"))
}
