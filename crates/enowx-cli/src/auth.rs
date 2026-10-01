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

/// A Cloudflare API token scope the login flow can pre-fill on the token page.
/// `minimal` and `medium` are read-only: verifying control of a domain never
/// needs to write. `full` grants edit across every service and is far more
/// than an assessment needs, so it is offered with a warning.
struct CfScope {
    name: &'static str,
    summary: &'static str,
    /// The permission groups, as the token page's deep link expects them.
    groups: &'static str,
    /// Whether choosing it grants write access to the whole account.
    dangerous: bool,
}

const CF_SCOPES: [CfScope; 3] = [
    CfScope {
        name: "minimal",
        summary: "Zone:Read. Enough to prove control of a domain. Recommended.",
        groups: r#"[{"key":"zone","type":"read"}]"#,
        dangerous: false,
    },
    CfScope {
        name: "medium",
        summary: "Zone:Read + DNS:Read, so records resolve without a separate lookup.",
        groups: r#"[{"key":"zone","type":"read"},{"key":"dns_records","type":"read"}]"#,
        dangerous: false,
    },
    CfScope {
        name: "full",
        summary: "EDIT across every Cloudflare service. Full control of the account. Only if you need it.",
        // Every service enx can name, at edit. The deep link ignores a key it
        // does not know, so a service missing here is just not pre-ticked; add
        // it on the page. This is a powerful token, hence the warning.
        groups: r#"[{"key":"zone","type":"edit"},{"key":"dns_records","type":"edit"},{"key":"zone_settings","type":"edit"},{"key":"ssl_and_certificates","type":"edit"},{"key":"firewall_services","type":"edit"},{"key":"cache_purge","type":"edit"},{"key":"page_rules","type":"edit"},{"key":"load_balancing","type":"edit"},{"key":"waf","type":"edit"},{"key":"logs","type":"edit"},{"key":"analytics","type":"read"},{"key":"transform_rules","type":"edit"},{"key":"rate_limit","type":"edit"},{"key":"health_checks","type":"edit"},{"key":"dns_firewall","type":"edit"},{"key":"spectrum","type":"edit"},{"key":"workers_scripts","type":"edit"},{"key":"workers_routes","type":"edit"},{"key":"workers_kv_storage","type":"edit"},{"key":"pages","type":"edit"},{"key":"stream","type":"edit"},{"key":"images","type":"edit"},{"key":"access_apps","type":"edit"},{"key":"access_service_tokens","type":"edit"}]"#,
        dangerous: true,
    },
];

/// The Cloudflare "Create Custom Token" page, pre-filled with `scope`'s
/// permissions and a name, for all the account's zones. The user still reviews
/// and narrows it before creating the token.
fn cf_template_url(scope: &CfScope) -> String {
    let encode = |s: &str| {
        let mut out = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    };
    format!(
        "https://dash.cloudflare.com/profile/api-tokens?name={}&permissionGroupKeys={}&accountId=*&zoneId=all",
        encode("enx security assessment"),
        encode(scope.groups),
    )
}

/// Open a URL in the user's default browser. Best effort.
fn open_url(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let (bin, args): (&str, &[&str]) = ("open", &[]);
    #[cfg(target_os = "linux")]
    let (bin, args): (&str, &[&str]) = ("xdg-open", &[]);
    #[cfg(target_os = "windows")]
    let (bin, args): (&str, &[&str]) = ("cmd", &["/C", "start", ""]);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let (bin, args): (&str, &[&str]) = ("", &[]);
    if bin.is_empty() {
        return false;
    }
    std::process::Command::new(bin)
        .args(args)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Ask which scope, open the token page pre-filled, read the token, verify it
/// reads zones, and store it. `cloudflare` is a connected account enx uses to
/// prove control of a domain before a security assessment, not a model
/// provider, so it is stored under its own id.
fn login_cloudflare(config: &mut Config) -> Result<()> {
    eprintln!("Connect a Cloudflare account so enx can prove you control a domain before");
    eprintln!("testing it. Pick the token scope (minimal and medium are read-only):\n");
    for (i, scope) in CF_SCOPES.iter().enumerate() {
        eprintln!("  {}. {} - {}", i + 1, scope.name, scope.summary);
    }
    let scope = if std::io::stdin().is_terminal() {
        eprint!("\nScope [1]: ");
        std::io::stderr().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        match line.trim() {
            "2" | "medium" => &CF_SCOPES[1],
            "3" | "full" => &CF_SCOPES[2],
            _ => &CF_SCOPES[0],
        }
    } else {
        &CF_SCOPES[0]
    };

    if scope.dangerous {
        eprintln!("\n  WARNING: the full scope creates a token with EDIT access to every");
        eprintln!("  Cloudflare service, full control of the account. enx only needs to READ");
        eprintln!("  zones to verify you control a domain; it never writes. A leaked full");
        eprintln!("  token lets anyone change DNS, WAF, Workers, Pages and more. Prefer");
        eprintln!("  minimal unless you have a reason, and give it no expiry only knowingly.");
        if std::io::stdin().is_terminal() {
            eprint!("  Create a full-access token anyway? [y/N]: ");
            std::io::stderr().flush()?;
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            anyhow::ensure!(
                matches!(line.trim(), "y" | "Y" | "yes"),
                "cancelled; run again and pick minimal"
            );
        }
    }

    let url = cf_template_url(scope);
    eprintln!(
        "\nOpening the Cloudflare token page with the {} template pre-filled.",
        scope.name
    );
    eprintln!("Review it (narrow it to the specific zones if you like), create the token,");
    eprintln!("and copy it. For the longest-lived token, leave \"Token valid until\" as no");
    eprintln!("expiry (that outlasts any end date); leave the start date as today.");
    eprintln!("If the page did not open, go to:\n{url}\n");
    if !open_url(&url) {
        eprintln!("(could not open a browser automatically; use the link above)");
    }

    let key = read_key("Cloudflare API token: ")?;
    anyhow::ensure!(!key.trim().is_empty(), "no token entered; nothing saved");

    match verify_cloudflare_token(key.trim()) {
        Ok(count) => println!("Token verified: it reads {count} zone(s)."),
        Err(error) => {
            eprintln!("Warning: could not verify the token now ({error}).");
            if std::io::stdin().is_terminal() {
                eprint!("Save it anyway? [y/N]: ");
                std::io::stderr().flush()?;
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line)?;
                anyhow::ensure!(
                    matches!(line.trim(), "y" | "Y" | "yes"),
                    "not saved; connect again when the token is ready"
                );
            }
        }
    }

    config.auth.store("cloudflare", key.trim())?;
    println!(
        "Saved the Cloudflare token to {}. The security team can now verify control of a \
         domain before testing it.",
        enowx_core::auth::auth_path().display()
    );
    Ok(())
}

/// Confirm a token reads zones, returning how many. Runs the core's async
/// check on the ambient tokio runtime, so the flow can reject a bad token
/// before saving it.
fn verify_cloudflare_token(token: &str) -> Result<usize> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(enowx_core::tools::authorize::verify_token(token))
    })
}

/// Ask for `provider`'s key and store it. `cloudflare` is not a model provider
/// but a connected account enx uses to verify control of a domain before a
/// security assessment, so it is stored the same way under its own id.
pub fn login(config: &mut Config, provider: &str) -> Result<()> {
    if provider.eq_ignore_ascii_case("cloudflare") {
        return login_cloudflare(config);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_url_pre_fills_the_scope() {
        let url = cf_template_url(&CF_SCOPES[0]);
        assert!(url.starts_with("https://dash.cloudflare.com/profile/api-tokens?"));
        assert!(url.contains("name=enx%20security%20assessment"));
        // The minimal scope is Zone:Read, url-encoded.
        assert!(url.contains("permissionGroupKeys=%5B%7B%22key%22%3A%22zone%22"));
        assert!(url.contains("zoneId=all"));
        // minimal and medium are read-only; only full grants edit, and it is
        // the only one marked dangerous.
        for scope in CF_SCOPES {
            if scope.dangerous {
                assert!(scope.groups.contains("\"edit\""), "full grants edit");
            } else {
                assert!(
                    !scope.groups.contains("\"edit\""),
                    "{} is read-only",
                    scope.name
                );
            }
        }
        assert_eq!(CF_SCOPES.iter().filter(|s| s.dangerous).count(), 1);
    }
}
