//! `enx auth`: provider keys in `~/.enx/auth.json`, the way `opencode auth`
//! manages them. A key is typed at a prompt that does not echo it, or piped
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

/// Ask for `provider`'s key and store it.
pub fn login(config: &mut Config, provider: &str) -> Result<()> {
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
