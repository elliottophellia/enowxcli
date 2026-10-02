//! The user's VPSes over SSH: run a command on one, or read its state.
//!
//! Each host signs in with a private key file or a password kept in
//! `auth.json`. A host's key is recorded the first time enx connects
//! (`~/.enx/vps_known_hosts`) and a later connection to a different key is
//! refused, as OpenSSH does, so a hijacked address never sees a password.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{bail, Context as _, Result};
use russh::{
    client,
    keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg},
    ChannelMsg,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{arg, render, schema, secret_id, Server, ToolSpec};
use crate::{auth::Auth, config::home_dir};

/// One VPS. Its password, when it signs in with one, is in `auth.json`
/// under [`password_id`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Host {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    /// A private key file. None: sign in with the stored password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
}

fn default_port() -> u16 {
    22
}

/// Where `name`'s password is kept in `auth.json`.
pub fn password_id(name: &str) -> String {
    secret_id(&format!("vps-{name}"))
}

/// A VPS name as `enx vps add` accepts it.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn known_hosts_path() -> PathBuf {
    home_dir().join("vps_known_hosts")
}

/// `host:port` to the SHA-256 fingerprint of the key it presented first.
fn known_hosts() -> BTreeMap<String, String> {
    std::fs::read_to_string(known_hosts_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn remember_host(address: &str, fingerprint: &str) -> Result<()> {
    let mut hosts = known_hosts();
    hosts.insert(address.to_owned(), fingerprint.to_owned());
    let path = known_hosts_path();
    crate::config::atomic_write(&path, serde_json::to_string_pretty(&hosts)?.as_bytes())?;
    Ok(())
}

/// Accepts a host's key the first time, then only that key.
struct Verifier {
    address: String,
    /// Set when the key differs from the recorded one, to say why.
    mismatch: Arc<std::sync::Mutex<Option<String>>>,
}

impl client::Handler for Verifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        match known_hosts().get(&self.address) {
            Some(known) if *known == fingerprint => Ok(true),
            Some(known) => {
                *self.mismatch.lock().unwrap() = Some(format!(
                    "{} presented host key {fingerprint}, not the {known} enx recorded. \
                     If the server was rebuilt, remove its line from {} and connect again.",
                    self.address,
                    known_hosts_path().display()
                ));
                Ok(false)
            }
            None => {
                let _ = remember_host(&self.address, &fingerprint);
                Ok(true)
            }
        }
    }
}

/// What a command printed and how it ended.
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub exit: Option<u32>,
}

/// Run `command` on `host` and collect its output, within `limit`.
pub async fn run(
    name: &str,
    host: &Host,
    auth: &Auth,
    command: &str,
    limit: Duration,
) -> Result<Output> {
    let address = format!("{}:{}", host.host, host.port);
    let mismatch = Arc::new(std::sync::Mutex::new(None));
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(limit + Duration::from_secs(5)),
        ..Default::default()
    });
    let verifier = Verifier {
        address: address.clone(),
        mismatch: mismatch.clone(),
    };
    let connecting = client::connect(config, (host.host.as_str(), host.port), verifier);
    let mut session = match tokio::time::timeout(Duration::from_secs(20), connecting).await {
        Err(_) => bail!("{name}: no answer from {address} within 20 seconds"),
        Ok(Err(error)) => {
            if let Some(why) = mismatch.lock().unwrap().take() {
                bail!("{name}: refused: {why}");
            }
            return Err(error).with_context(|| format!("{name}: connecting to {address}"));
        }
        Ok(Ok(session)) => session,
    };
    let signed_in = match &host.key_path {
        Some(path) => {
            let path = expand_home(path);
            let key = load_secret_key(&path, None)
                .with_context(|| format!("{name}: reading the key {}", path.display()))?;
            let hash = session.best_supported_rsa_hash().await?.flatten();
            session
                .authenticate_publickey(&host.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await?
                .success()
        }
        None => {
            let (password, _) = auth.key(&password_id(name), &[]).with_context(|| {
                format!(
                    "{name}: no key file and no password stored; run `enx vps add {name}` again"
                )
            })?;
            session
                .authenticate_password(&host.user, password)
                .await?
                .success()
        }
    };
    if !signed_in {
        bail!(
            "{name}: {address} refused {} as {}",
            if host.key_path.is_some() {
                "the key"
            } else {
                "the password"
            },
            host.user
        );
    }
    let mut channel = session.channel_open_session().await?;
    channel.exec(true, command).await?;
    let collect = async {
        let (mut stdout, mut stderr, mut exit) = (Vec::new(), Vec::new(), None);
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
                ChannelMsg::ExtendedData { ref data, .. } => stderr.extend_from_slice(data),
                ChannelMsg::ExitStatus { exit_status } => exit = Some(exit_status),
                _ => {}
            }
        }
        (stdout, stderr, exit)
    };
    let (stdout, stderr, exit) = tokio::time::timeout(limit, collect).await.map_err(|_| {
        anyhow::anyhow!(
            "{name}: the command ran past {} seconds and was abandoned",
            limit.as_secs()
        )
    })?;
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;
    Ok(Output {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit,
    })
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest),
        None => PathBuf::from(path),
    }
}

/// Command output cut to what a model can read, keeping the end, where
/// errors are.
fn tail(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut start = text.len() - limit;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("… ({} bytes cut)\n{}", start, &text[start..])
}

/// A snapshot of a host's state, read-only.
const STATUS: &str = "echo '== host'; hostname; uptime; \
    echo; echo '== disk'; df -h -x tmpfs -x devtmpfs -x overlay -x efivarfs 2>/dev/null || df -h; \
    echo; echo '== memory'; free -m 2>/dev/null || vm_stat; \
    echo; echo '== docker'; \
    (docker ps --format 'table {{.Names}}\\t{{.Status}}\\t{{.Ports}}' 2>/dev/null || echo 'docker not available')";

pub struct Vps {
    hosts: BTreeMap<String, Host>,
    auth: Auth,
}

impl Vps {
    pub fn new(hosts: BTreeMap<String, Host>, auth: Auth) -> Self {
        Self { hosts, auth }
    }

    fn host(&self, name: &str) -> Result<&Host> {
        self.hosts.get(name).with_context(|| {
            format!(
                "no VPS named `{name}`; set up: {}",
                self.hosts.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })
    }
}

#[async_trait::async_trait]
impl Server for Vps {
    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "list",
                description: "The VPSes set up in enx: name, address, user, and how each signs in. No secrets.",
                input_schema: schema(&[], &[]),
            },
            ToolSpec {
                name: "exec",
                description: "Run one shell command on a VPS over SSH and return its output and exit code. It runs as the configured user with that user's full rights: prefer reading over changing, and say what a command will change before running it.",
                input_schema: schema(
                    &[
                        ("host", "The VPS name, from list"),
                        ("command", "The shell command to run"),
                        ("timeout", "Seconds to allow, default 60, at most 600"),
                    ],
                    &["host", "command"],
                ),
            },
            ToolSpec {
                name: "status",
                description: "A read-only snapshot of a VPS: uptime and load, disk, memory, and running Docker containers.",
                input_schema: schema(&[("host", "The VPS name, from list")], &["host"]),
            },
        ]
    }

    async fn call(&self, tool: &str, args: &Value) -> Result<String> {
        match tool {
            "list" => Ok(render(&Value::Array(
                self.hosts
                    .iter()
                    .map(|(name, host)| {
                        json!({
                            "name": name,
                            "address": format!("{}:{}", host.host, host.port),
                            "user": host.user,
                            "signs_in_with": if host.key_path.is_some() { "key file" } else { "password" },
                        })
                    })
                    .collect(),
            ))),
            "exec" | "status" => {
                let name = arg(args, "host")?;
                let host = self.host(name)?;
                let (command, seconds) = if tool == "status" {
                    (STATUS, 60)
                } else {
                    let seconds = args
                        .get("timeout")
                        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                        .unwrap_or(60)
                        .clamp(1, 600);
                    (arg(args, "command")?, seconds)
                };
                let output = run(name, host, &self.auth, command, Duration::from_secs(seconds)).await?;
                let mut text = tail(&output.stdout, 40_000);
                if !output.stderr.trim().is_empty() {
                    text.push_str(&format!("\n[stderr]\n{}", tail(&output.stderr, 10_000)));
                }
                text.push_str(&match output.exit {
                    Some(code) => format!("\n[exit {code}]"),
                    None => "\n[no exit status]".to_owned(),
                });
                Ok(text)
            }
            other => bail!("vps has no tool `{other}`"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_short_and_plain() {
        assert!(valid_name("prod-1"));
        assert!(valid_name("oracle_arm"));
        assert!(!valid_name(""));
        assert!(!valid_name("a b"));
        assert!(!valid_name("../x"));
    }

    #[tokio::test]
    async fn the_list_shows_no_secrets() {
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod".to_owned(),
            Host {
                host: "203.0.113.5".into(),
                port: 22,
                user: "root".into(),
                key_path: None,
            },
        );
        let vps = Vps::new(hosts, Auth::default());
        let listed = vps.call("list", &json!({})).await.unwrap();
        assert!(listed.contains("203.0.113.5:22"), "{listed}");
        assert!(listed.contains("password"), "{listed}");
        assert!(vps
            .call("exec", &json!({"host":"nope","command":"ls"}))
            .await
            .is_err());
    }

    #[test]
    fn long_output_keeps_its_end() {
        let text = format!("{}END", "x".repeat(100));
        let cut = tail(&text, 10);
        assert!(cut.ends_with("END"));
        assert!(cut.starts_with("… ("));
    }
}
