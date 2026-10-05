//! The user's VPSes over SSH: run a command on one, or read its state.
//!
//! A host signs in the way OpenSSH would, trying in turn: the key file it
//! was given (decrypted with its stored passphrase), the keys in ssh-agent
//! (Pageant or the OpenSSH agent on Windows), the `IdentityFile`s from
//! `~/.ssh/config` or else the default `~/.ssh/id_*` keys, the stored
//! password, and keyboard-interactive answered with that password. A host
//! may also be an alias from `~/.ssh/config`, whose `HostName`, `User` and
//! `Port` are used. Secrets live in `auth.json`.
//!
//! A host's key is recorded the first time enx connects
//! (`~/.enx/vps_known_hosts`) and a later connection to a different key is
//! refused, as OpenSSH does, so a hijacked address never sees a password.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{bail, Context as _, Result};
use russh::{
    client::{self, KeyboardInteractiveAuthResponse},
    keys::{
        agent::{client::AgentClient, AgentIdentity},
        load_secret_key, HashAlg, PrivateKeyWithHashAlg,
    },
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
    /// An address, or an alias from `~/.ssh/config`.
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Empty: the `User` from `~/.ssh/config`, or the local user name.
    #[serde(default)]
    pub user: String,
    /// A private key file. None: ssh-agent, the keys `~/.ssh/config` names
    /// or the default ones, then the stored password.
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

/// Where the passphrase of `name`'s key file is kept in `auth.json`.
pub fn passphrase_id(name: &str) -> String {
    secret_id(&format!("vps-{name}-passphrase"))
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
                    "{} presented host key {fingerprint}, not the {known} enowx recorded. \
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

/// What `~/.ssh/config` says about one host alias.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SshEntry {
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_files: Vec<String>,
}

/// Read the settings `text` (an ssh_config) gives `alias`. Like OpenSSH,
/// the first value found for a keyword wins and `IdentityFile`s add up;
/// `Match` blocks are skipped, since they test things enx cannot know.
pub fn ssh_config_entry(text: &str, alias: &str) -> SshEntry {
    let mut entry = SshEntry::default();
    let mut active = true;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.find(|c: char| c.is_whitespace() || c == '=') {
            Some(at) => (
                &line[..at],
                line[at..]
                    .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
                    .trim(),
            ),
            None => (line, ""),
        };
        let value = value.trim_matches('"');
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                let patterns: Vec<&str> = value.split_whitespace().collect();
                let negated = patterns
                    .iter()
                    .filter_map(|p| p.strip_prefix('!'))
                    .any(|p| glob(p, alias));
                active = !negated
                    && patterns
                        .iter()
                        .filter(|p| !p.starts_with('!'))
                        .any(|p| glob(p, alias));
            }
            "match" => active = false,
            _ if !active => {}
            "hostname" if entry.host_name.is_none() => {
                entry.host_name = Some(value.replace("%h", alias));
            }
            "user" if entry.user.is_none() => entry.user = Some(value.to_owned()),
            "port" if entry.port.is_none() => entry.port = value.parse().ok(),
            "identityfile" => entry.identity_files.push(value.to_owned()),
            _ => {}
        }
    }
    entry
}

/// An ssh_config host pattern: `*` any run, `?` one character.
fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|i| go(rest, &t[i..])),
            Some(('?', rest)) => !t.is_empty() && go(rest, &t[1..]),
            Some((c, rest)) => {
                t.first().is_some_and(|f| f.eq_ignore_ascii_case(c)) && go(rest, &t[1..])
            }
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    go(&p, &t)
}

/// The user's home: `HOME`, or `USERPROFILE` on Windows.
fn user_home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Where a host is reached and as whom, once `~/.ssh/config` is applied.
struct Target {
    host: String,
    port: u16,
    user: String,
    /// Key files to try after the agent: the config's, or the defaults.
    identity_files: Vec<PathBuf>,
}

fn resolve(host: &Host) -> Target {
    let text = std::fs::read_to_string(user_home().join(".ssh").join("config")).unwrap_or_default();
    let entry = ssh_config_entry(&text, &host.host);
    let user = if host.user.trim().is_empty() {
        entry.user.clone().unwrap_or_else(|| {
            std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "root".into())
        })
    } else {
        host.user.clone()
    };
    let identity_files = if entry.identity_files.is_empty() {
        ["id_ed25519", "id_ecdsa", "id_rsa"]
            .iter()
            .map(|f| user_home().join(".ssh").join(f))
            .collect()
    } else {
        entry
            .identity_files
            .iter()
            .map(|f| expand_home(f))
            .collect()
    };
    Target {
        host: entry.host_name.unwrap_or_else(|| host.host.clone()),
        // An explicit port wins; 22 is the default and yields to the config.
        port: if host.port != 22 {
            host.port
        } else {
            entry.port.unwrap_or(22)
        },
        user,
        identity_files,
    }
}

/// How a host will sign in, for `list` and `enx vps list`. No secrets.
pub fn sign_in_summary(name: &str, host: &Host, auth: &Auth) -> String {
    let mut ways = Vec::new();
    if let Some(key) = &host.key_path {
        ways.push(format!("key {key}"));
    }
    ways.push("ssh-agent and ~/.ssh keys".to_owned());
    if auth.key(&password_id(name), &[]).is_some() {
        ways.push("password".to_owned());
    }
    ways.join(", then ")
}

/// The running ssh-agent, if there is one.
async fn agent(
) -> Option<AgentClient<Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>>> {
    let connect = async {
        #[cfg(unix)]
        {
            AgentClient::connect_env().await.ok().map(|a| a.dynamic())
        }
        #[cfg(windows)]
        {
            if let Ok(a) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
                return Some(a.dynamic());
            }
            AgentClient::connect_pageant()
                .await
                .ok()
                .map(|a| a.dynamic())
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    };
    tokio::time::timeout(Duration::from_secs(3), connect)
        .await
        .ok()
        .flatten()
}

/// Whether a keyboard-interactive prompt asks for the account password,
/// rather than a one-time code enx has no way to give.
fn asks_for_password(prompt: &russh::client::Prompt) -> bool {
    let text = prompt.prompt.to_lowercase();
    !prompt.echo
        && ![
            "code",
            "otp",
            "token",
            "verification",
            "2fa",
            "authenticator",
            "yubikey",
        ]
        .iter()
        .any(|w| text.contains(w))
}

/// Servers drop the connection after a handful of refused keys
/// (`MaxAuthTries`, 6 by default), before the password gets its turn.
const MAX_KEYS: usize = 4;

/// Sign in the ways OpenSSH would, stopping at the first that works.
/// On failure, says what was tried and why each was refused.
async fn sign_in(
    session: &mut client::Handle<Verifier>,
    name: &str,
    host: &Host,
    target: &Target,
    auth: &Auth,
) -> Result<()> {
    let user = target.user.as_str();
    let password = auth.key(&password_id(name), &[]).map(|(k, _)| k.to_owned());
    let passphrase = auth
        .key(&passphrase_id(name), &[])
        .map(|(k, _)| k.to_owned());
    let hash = session.best_supported_rsa_hash().await?.flatten();
    let mut tried: Vec<String> = Vec::new();
    let mut offered = 0;
    let mut seen = std::collections::HashSet::new();

    // 1. The key file it was given.
    let explicit = host.key_path.as_deref().map(expand_home);
    if let Some(path) = &explicit {
        match load_key(path, passphrase.as_deref()) {
            Ok(key) => {
                seen.insert(key.public_key().to_bytes().unwrap_or_default());
                offered += 1;
                let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
                if session.authenticate_publickey(user, key).await?.success() {
                    return Ok(());
                }
                tried.push(format!("key {} refused", path.display()));
            }
            Err(why) => tried.push(format!("key {}: {why}", path.display())),
        }
    }

    // 2. ssh-agent.
    if let Some(mut agent) = agent().await {
        let identities = agent.request_identities().await.unwrap_or_default();
        let mut refused = 0;
        for identity in identities {
            if offered >= MAX_KEYS {
                break;
            }
            let AgentIdentity::PublicKey { key, .. } = identity else {
                continue;
            };
            if !seen.insert(key.to_bytes().unwrap_or_default()) {
                continue;
            }
            offered += 1;
            match session
                .authenticate_publickey_with(user, key, hash, &mut agent)
                .await
            {
                Ok(result) if result.success() => return Ok(()),
                Ok(_) => refused += 1,
                Err(error) => tried.push(format!("ssh-agent: {error:?}")),
            }
        }
        if refused > 0 {
            tried.push(format!("{refused} ssh-agent key(s) refused"));
        }
    }

    // 3. The config's key files, or the default ones.
    for path in &target.identity_files {
        if offered >= MAX_KEYS || explicit.as_ref() == Some(path) || !path.is_file() {
            continue;
        }
        let key = match load_key(path, passphrase.as_deref()) {
            Ok(key) => key,
            Err(why) => {
                tried.push(format!("key {}: {why}", path.display()));
                continue;
            }
        };
        if !seen.insert(key.public_key().to_bytes().unwrap_or_default()) {
            continue;
        }
        offered += 1;
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
        if session.authenticate_publickey(user, key).await?.success() {
            return Ok(());
        }
        tried.push(format!("key {} refused", path.display()));
    }

    // 4. The password, then keyboard-interactive answered with it: servers
    // that turn off plain password sign-in often still ask this way.
    if let Some(password) = &password {
        if session
            .authenticate_password(user, password.clone())
            .await?
            .success()
        {
            return Ok(());
        }
        tried.push("password refused".into());
        let mut reply = session
            .authenticate_keyboard_interactive_start(user, None)
            .await?;
        let mut answered = 0;
        loop {
            match reply {
                KeyboardInteractiveAuthResponse::Success => return Ok(()),
                KeyboardInteractiveAuthResponse::Failure { .. } => break,
                KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                    if let Some(other) = prompts.iter().find(|p| !asks_for_password(p)) {
                        tried.push(format!(
                            "keyboard-interactive asked {:?}, which enowx cannot answer; sign in with a key instead",
                            other.prompt.trim()
                        ));
                        break;
                    }
                    if !prompts.is_empty() {
                        answered += 1;
                    }
                    // The same password twice is the wrong password.
                    if answered > 1 {
                        tried.push("keyboard-interactive refused the password".into());
                        break;
                    }
                    let answers = prompts.iter().map(|_| password.clone()).collect();
                    reply = session
                        .authenticate_keyboard_interactive_respond(answers)
                        .await?;
                }
            }
        }
    }

    if tried.is_empty() {
        bail!(
            "{name}: nothing to sign in to {}@{} with: no key file, no ssh-agent key, no ~/.ssh key and no password. \
             Run `enowx vps add {name} --key <file>` or `--password <password>`.",
            user,
            target.host
        );
    }
    bail!(
        "{name}: {}@{}:{} refused every way enowx tried: {}",
        user,
        target.host,
        target.port,
        tried.join("; ")
    )
}

/// Whether a key file is encrypted; an error if it is not a key enx reads.
pub fn key_is_encrypted(path: &std::path::Path) -> Result<bool> {
    match load_secret_key(path, None) {
        Ok(_) => Ok(false),
        Err(russh::keys::Error::KeyIsEncrypted) => Ok(true),
        Err(error) => Err(error.into()),
    }
}

/// Whether `passphrase` opens the key file.
pub fn key_opens(path: &std::path::Path, passphrase: &str) -> bool {
    load_secret_key(path, Some(passphrase)).is_ok()
}

/// A key file, decrypted with `passphrase` when it is encrypted.
fn load_key(
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> Result<russh::keys::PrivateKey, String> {
    match load_secret_key(path, None) {
        Ok(key) => Ok(key),
        Err(russh::keys::Error::KeyIsEncrypted) => match passphrase {
            Some(passphrase) => load_secret_key(path, Some(passphrase))
                .map_err(|_| "the stored passphrase does not open it".to_owned()),
            None => {
                Err("encrypted, and no passphrase stored (enowx vps add ... --passphrase)".into())
            }
        },
        Err(error) => Err(error.to_string()),
    }
}

/// Run `command` on `host` and collect its output, within `limit`.
pub async fn run(
    name: &str,
    host: &Host,
    auth: &Auth,
    command: &str,
    limit: Duration,
) -> Result<Output> {
    let target = resolve(host);
    let address = format!("{}:{}", target.host, target.port);
    let mismatch = Arc::new(std::sync::Mutex::new(None));
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(limit + Duration::from_secs(5)),
        ..Default::default()
    });
    let verifier = Verifier {
        address: address.clone(),
        mismatch: mismatch.clone(),
    };
    let connecting = client::connect(config, (target.host.as_str(), target.port), verifier);
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
    sign_in(&mut session, name, host, &target, auth).await?;
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
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => user_home().join(rest),
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
                description: "The VPSes set up in enowx: name, address, user, and how each signs in. No secrets.",
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
                            "signs_in_with": sign_in_summary(name, host, &self.auth),
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
                        .and_then(|v| {
                            v.as_u64()
                                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                        })
                        .unwrap_or(60)
                        .clamp(1, 600);
                    (arg(args, "command")?, seconds)
                };
                let output = run(
                    name,
                    host,
                    &self.auth,
                    command,
                    Duration::from_secs(seconds),
                )
                .await?;
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
        assert!(listed.contains("ssh-agent"), "{listed}");
        assert!(vps
            .call("exec", &json!({"host":"nope","command":"ls"}))
            .await
            .is_err());
    }

    const CONFIG: &str = "
Host *.internal !secret.internal
    User ops
Host prod web-?
    HostName 203.0.113.9
    User deploy
    Port 2222
    IdentityFile ~/.ssh/prod_ed25519
Match host foo
    User nobody
Host *
    User fallback
    IdentityFile ~/.ssh/id_rsa
";

    #[test]
    fn an_ssh_config_alias_is_resolved() {
        let prod = ssh_config_entry(CONFIG, "prod");
        assert_eq!(prod.host_name.as_deref(), Some("203.0.113.9"));
        assert_eq!(prod.user.as_deref(), Some("deploy"), "the first User wins");
        assert_eq!(prod.port, Some(2222));
        assert_eq!(
            prod.identity_files,
            ["~/.ssh/prod_ed25519", "~/.ssh/id_rsa"],
            "IdentityFiles add up"
        );
        assert_eq!(ssh_config_entry(CONFIG, "web-1").port, Some(2222));
        assert_eq!(
            ssh_config_entry(CONFIG, "web-10").port,
            None,
            "? is one character"
        );
        assert_eq!(
            ssh_config_entry(CONFIG, "db.internal").user.as_deref(),
            Some("ops")
        );
        assert_eq!(
            ssh_config_entry(CONFIG, "secret.internal").user.as_deref(),
            Some("fallback"),
            "a negated pattern excludes"
        );
        assert_eq!(
            ssh_config_entry(CONFIG, "foo").user.as_deref(),
            Some("fallback"),
            "Match is skipped"
        );
    }

    #[test]
    fn key_value_with_equals_and_quotes() {
        let entry = ssh_config_entry("Host x\n  HostName=\"10.0.0.1\"\n  Port = 2200\n", "x");
        assert_eq!(entry.host_name.as_deref(), Some("10.0.0.1"));
        assert_eq!(entry.port, Some(2200));
    }

    #[test]
    fn a_one_time_code_is_not_answered_with_the_password() {
        let ask = |prompt: &str, echo| russh::client::Prompt {
            prompt: prompt.into(),
            echo,
        };
        assert!(asks_for_password(&ask("Password: ", false)));
        assert!(asks_for_password(&ask("root@host's password:", false)));
        assert!(!asks_for_password(&ask("Verification code: ", false)));
        assert!(!asks_for_password(&ask("Username: ", true)));
    }

    /// A throwaway ed25519 key made for this test, encrypted with `hunter2`.
    #[test]
    fn an_encrypted_key_needs_its_passphrase() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/encrypted_ed25519");
        assert!(load_key(&path, None).unwrap_err().contains("passphrase"));
        assert!(load_key(&path, Some("wrong")).is_err());
        assert!(load_key(&path, Some("hunter2")).is_ok());
    }

    #[test]
    fn long_output_keeps_its_end() {
        let text = format!("{}END", "x".repeat(100));
        let cut = tail(&text, 10);
        assert!(cut.ends_with("END"));
        assert!(cut.starts_with("… ("));
    }
}
