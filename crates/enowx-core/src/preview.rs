//! Looking at a page in a real browser.
//!
//! Every interface an agent built was reported "not checked in a browser;
//! checked by reading the CSS". This opens the page in headless Chrome at a
//! phone, a tablet and a wide width, measures what a person would see
//! (horizontal overflow, text contrast, links to nowhere, images without alt,
//! controls without a name, small touch targets, console errors) and saves a
//! screenshot of each width. It talks to Chrome over the DevTools protocol,
//! and starts and stops a dev server when the page needs one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::tungstenite::Message;

/// The widths a page is looked at: a small phone, a tablet, a laptop.
pub const WIDTHS: [u32; 3] = [360, 768, 1440];

/// The tallest screenshot saved; a longer page is cut there.
const MAX_SHOT_HEIGHT: u32 = 6000;

/// What is looked at.
pub enum Target {
    /// A file in the workspace, opened as `file://`.
    File(PathBuf),
    /// A page served somewhere, usually a dev server.
    Url(String),
}

/// Signing in before looking, for pages behind a sign-in: the form at `url`
/// is filled and submitted in the same browser, so its cookies and storage
/// carry over to the page looked at.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Login {
    /// The sign-in page.
    pub url: String,
    /// Each field, by its name, id, label or placeholder, and the value to
    /// type: `{"username": "admin", "password": "admin123"}`.
    pub fields: std::collections::BTreeMap<String, String>,
    /// The submit button's text; the form's submit button when empty.
    #[serde(default)]
    pub submit: Option<String>,
}

/// What one width showed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WidthReport {
    pub width: u32,
    pub page_height: u64,
    pub overflow_px: i64,
    pub overflowing: Vec<String>,
    pub low_contrast: Vec<String>,
    pub dead_anchors: Vec<String>,
    pub missing_alt: Vec<String>,
    pub unnamed_controls: Vec<String>,
    pub small_targets: Vec<String>,
    pub small_text: u64,
    pub h1_count: u64,
    /// Whether the page has `<meta name="viewport">`; without it a phone
    /// lays the page out at 980px and shrinks it.
    pub viewport_meta: bool,
    /// How many screens tall the page is at this width.
    pub screens: f64,
    /// A page longer than a screen and a half whose top bar leaves the
    /// screen when it is scrolled.
    pub header_scrolls_away: bool,
    /// A page longer than three screens with no visible way back to the top
    /// from halfway down.
    pub no_back_to_top: bool,
    /// Four or more tall blocks in a row built the same way: the same module
    /// repeated down the page, e.g. `article.project ×7`.
    pub repeated_blocks: Option<String>,
    /// The words in the page's first `h1`, when there are too many for a
    /// headline.
    pub long_headline: Option<usize>,
    /// A side column (a sidebar or a nav) whose background stops before the
    /// bottom of the window, as `aside.sidebar ends at 900px`.
    pub short_side: Option<String>,
    /// The same filled button repeated down the page, as `5 filled buttons
    /// in #e08a70`: the page's one primary action on every row.
    pub repeated_primary: Option<String>,
    /// The page background, when it sits in the grey middle ground: a dark
    /// theme that is charcoal rather than dark, or a light one that is dull
    /// grey. As `#1e1e1e, 12% light`.
    pub grey_background: Option<String>,
    /// An application screen whose content floats centred beside its
    /// sidebar on a wide screen, as `210px from the sidebar and 226px from
    /// the right edge on a 2400px screen`.
    pub floating_content: Option<String>,
    /// A sidebar still beside the content at a phone or tablet width, as
    /// `aside takes 224 of 360px`.
    pub open_sidebar: Option<String>,
    pub console_errors: Vec<String>,
    pub screenshot: Option<PathBuf>,
}

impl WidthReport {
    /// How many problems this width showed.
    pub fn problems(&self) -> usize {
        usize::from(self.overflow_px > 1)
            + self.low_contrast.len()
            + self.dead_anchors.len()
            + self.missing_alt.len()
            + self.unnamed_controls.len()
            + self.small_targets.len()
            + self.console_errors.len()
            + usize::from(self.h1_count != 1)
            + usize::from(!self.viewport_meta)
            + usize::from(self.header_scrolls_away)
            + usize::from(self.no_back_to_top)
            + usize::from(self.repeated_blocks.is_some())
            + usize::from(self.long_headline.is_some())
            + usize::from(self.grey_background.is_some())
            + usize::from(self.short_side.is_some())
            + usize::from(self.repeated_primary.is_some())
            + usize::from(self.floating_content.is_some())
            + usize::from(self.open_sidebar.is_some())
    }
}

/// Chrome, Chromium, Edge or Brave on this machine: `ENX_CHROME` first.
pub fn find_chrome() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ENX_CHROME").map(PathBuf::from) {
        if path.exists() {
            return Some(path);
        }
    }
    const CANDIDATES: &[&str] = &[
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/microsoft-edge",
        "/snap/bin/chromium",
    ];
    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

/// Open `target` at each width and report what it showed. `start` is a
/// command that serves the page (run in `workspace`, stopped afterwards).
pub async fn preview(
    workspace: &Path,
    target: Target,
    start: Option<&str>,
    out_dir: &Path,
) -> Result<Vec<WidthReport>> {
    preview_signed_in(workspace, target, start, None, out_dir).await
}

/// `preview`, signing in first with `login` when the page is behind one.
pub async fn preview_signed_in(
    workspace: &Path,
    target: Target,
    start: Option<&str>,
    login: Option<&Login>,
    out_dir: &Path,
) -> Result<Vec<WidthReport>> {
    let chrome = find_chrome().context(
        "no Chrome, Chromium, Edge or Brave found; install one, or set ENX_CHROME to its path",
    )?;
    let (url, start) = match target {
        Target::File(path) => {
            let path = if path.is_dir() {
                path.join("index.html")
            } else {
                path
            };
            let html = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "html" | "htm"));
            if !html {
                bail!(
                    "{} is not an HTML file; for an application with a build step, give the \
                     url its dev server serves and `start`, the command that runs it",
                    path.display()
                );
            }
            let url = reqwest::Url::from_file_path(&path)
                .map_err(|()| anyhow::anyhow!("{} is not an absolute path", path.display()))?;
            // A file is opened as it is; there is nothing to start.
            (url.to_string(), None)
        }
        Target::Url(url) => (url, start.filter(|command| !command.trim().is_empty())),
    };
    std::fs::create_dir_all(out_dir)?;
    let server = match start {
        Some(command) => Some(Server::start(workspace, command, &url, out_dir).await?),
        None => None,
    };
    let result = look(&chrome, &url, login, out_dir).await;
    if let Some(server) = server {
        server.stop().await;
    }
    result
}

async fn look(
    chrome: &Path,
    url: &str,
    login: Option<&Login>,
    out_dir: &Path,
) -> Result<Vec<WidthReport>> {
    let profile = std::env::temp_dir().join(format!("enx-chrome-{}", uuid::Uuid::new_v4()));
    let mut child = tokio::process::Command::new(chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--hide-scrollbars",
            "--mute-audio",
            "--remote-debugging-port=0",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("starting Chrome")?;
    let stderr = child.stderr.take().context("Chrome's stderr")?;
    let mut lines = BufReader::new(stderr).lines();
    let endpoint = tokio::time::timeout(Duration::from_secs(20), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(at) = line.find("ws://") {
                return Some(line[at..].trim().to_owned());
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .context("Chrome did not open its DevTools endpoint")?;
    // Chrome keeps logging; its stderr is read until it exits so it never
    // blocks on a full pipe.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    let outcome = async {
        let mut cdp = Cdp::connect(&endpoint).await?;
        let target = cdp
            .call("Target.createTarget", json!({"url": "about:blank"}), None)
            .await?;
        let target_id = target["targetId"]
            .as_str()
            .context("a target id")?
            .to_owned();
        let attached = cdp
            .call(
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
                None,
            )
            .await?;
        let session = attached["sessionId"]
            .as_str()
            .context("a session")?
            .to_owned();
        for method in ["Page.enable", "Runtime.enable", "Log.enable"] {
            cdp.call(method, json!({}), Some(&session)).await?;
        }
        if let Some(login) = login {
            cdp.sign_in(&session, login).await?;
        }
        let mut reports = Vec::new();
        for width in WIDTHS {
            reports.push(cdp.look_at(&session, url, width, out_dir).await?);
        }
        let _ = cdp.call("Browser.close", json!({}), None).await;
        Ok::<_, anyhow::Error>(reports)
    }
    .await;
    let _ = child.kill().await;
    let _ = std::fs::remove_dir_all(&profile);
    outcome
}

/// A minimal DevTools protocol client: calls by id, events kept for later.
struct Cdp {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
    events: Vec<Value>,
}

impl Cdp {
    async fn connect(endpoint: &str) -> Result<Self> {
        let (socket, _) = tokio_tungstenite::connect_async(endpoint)
            .await
            .context("connecting to Chrome")?;
        Ok(Self {
            socket,
            next_id: 0,
            events: Vec::new(),
        })
    }

    async fn call(&mut self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut message = json!({"id": id, "method": method, "params": params});
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        self.socket
            .send(Message::Text(message.to_string().into()))
            .await?;
        let wait = async {
            while let Some(frame) = self.socket.next().await {
                let Message::Text(text) = frame? else {
                    continue;
                };
                let value: Value = serde_json::from_str(&text)?;
                if value["id"].as_u64() == Some(id) {
                    if let Some(error) = value.get("error") {
                        bail!("{method}: {error}");
                    }
                    return Ok(value["result"].clone());
                }
                self.events.push(value);
            }
            bail!("Chrome closed the connection")
        };
        tokio::time::timeout(Duration::from_secs(60), wait)
            .await
            .with_context(|| format!("{method} timed out"))?
    }

    /// Wait for an event, looking first at those already received.
    async fn wait_for(&mut self, method: &str, session: &str, limit: Duration) -> bool {
        let seen = |value: &Value| value["method"] == method && value["sessionId"] == session;
        if let Some(at) = self.events.iter().position(seen) {
            self.events.remove(at);
            return true;
        }
        let wait = async {
            while let Some(frame) = self.socket.next().await {
                let Ok(Message::Text(text)) = frame else {
                    continue;
                };
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    if seen(&value) {
                        return true;
                    }
                    self.events.push(value);
                }
            }
            false
        };
        tokio::time::timeout(limit, wait).await.unwrap_or(false)
    }

    async fn look_at(
        &mut self,
        session: &str,
        url: &str,
        width: u32,
        out_dir: &Path,
    ) -> Result<WidthReport> {
        let mobile = width < 600;
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({"width": width, "height": if mobile { 780 } else { 900 },
                   "deviceScaleFactor": 1, "mobile": mobile}),
            Some(session),
        )
        .await?;
        self.events.clear();
        self.call("Page.navigate", json!({"url": url}), Some(session))
            .await?;
        // Measuring before the page loaded would measure the blank page it
        // replaced, and report its missing heading as the page's.
        if !self
            .wait_for("Page.loadEventFired", session, Duration::from_secs(45))
            .await
        {
            bail!("{url} did not finish loading within 45 seconds at {width}px");
        }
        // Late scripts and web fonts settle before anything is measured.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let result = self
            .call(
                "Runtime.evaluate",
                json!({"expression": CHECK_SCRIPT.replace("__WIDTH__", &width.to_string()),
                       "returnByValue": true, "awaitPromise": true}),
                Some(session),
            )
            .await?;
        let found = &result["result"]["value"];
        let strings = |key: &str| -> Vec<String> {
            found[key]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut report = WidthReport {
            width,
            page_height: found["height"].as_u64().unwrap_or(0),
            overflow_px: found["overflow_px"].as_i64().unwrap_or(0),
            overflowing: strings("overflowing"),
            low_contrast: strings("low_contrast"),
            dead_anchors: strings("dead_anchors"),
            missing_alt: strings("missing_alt"),
            unnamed_controls: strings("unnamed_controls"),
            small_targets: strings("small_targets"),
            small_text: found["small_text"].as_u64().unwrap_or(0),
            h1_count: found["h1"].as_u64().unwrap_or(0),
            viewport_meta: found["viewport_meta"].as_bool().unwrap_or(true),
            screens: found["screens"].as_f64().unwrap_or(0.0),
            header_scrolls_away: found["header_scrolls_away"].as_bool().unwrap_or(false),
            no_back_to_top: found["no_back_to_top"].as_bool().unwrap_or(false),
            repeated_blocks: found["repeated_blocks"].as_str().map(str::to_owned),
            long_headline: found["long_headline"].as_u64().map(|n| n as usize),
            grey_background: found["grey_background"].as_str().map(str::to_owned),
            short_side: found["short_side"].as_str().map(str::to_owned),
            repeated_primary: found["repeated_primary"].as_str().map(str::to_owned),
            floating_content: found["floating_content"].as_str().map(str::to_owned),
            open_sidebar: found["open_sidebar"].as_str().map(str::to_owned),
            console_errors: self.console_errors(session),
            screenshot: None,
        };
        let height = (report.page_height as u32).clamp(1, MAX_SHOT_HEIGHT);
        let shot = self
            .call(
                "Page.captureScreenshot",
                json!({"format": "png", "captureBeyondViewport": true,
                       "clip": {"x": 0, "y": 0, "width": width, "height": height, "scale": 1}}),
                Some(session),
            )
            .await?;
        if let Some(data) = shot["data"].as_str() {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
            let path = out_dir.join(format!("{width}.png"));
            std::fs::write(&path, bytes)?;
            report.screenshot = Some(path);
        }
        Ok(report)
    }

    /// Fill and submit the sign-in form, then check it went through: a page
    /// still showing a password field at the sign-in address did not.
    async fn sign_in(&mut self, session: &str, login: &Login) -> Result<()> {
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({"width": 1440, "height": 900, "deviceScaleFactor": 1, "mobile": false}),
            Some(session),
        )
        .await?;
        self.events.clear();
        self.call("Page.navigate", json!({"url": login.url}), Some(session))
            .await?;
        if !self
            .wait_for("Page.loadEventFired", session, Duration::from_secs(45))
            .await
        {
            bail!(
                "the sign-in page {} did not load within 45 seconds",
                login.url
            );
        }
        // A client-rendered form appears after the load event.
        tokio::time::sleep(Duration::from_millis(800)).await;
        let script = LOGIN_SCRIPT
            .replace("__FIELDS__", &serde_json::to_string(&login.fields)?)
            .replace("__SUBMIT__", &serde_json::to_string(&login.submit)?);
        let result = self
            .call(
                "Runtime.evaluate",
                json!({"expression": script, "returnByValue": true, "awaitPromise": true}),
                Some(session),
            )
            .await?;
        let found = &result["result"]["value"];
        let list = |key: &str| -> String {
            found[key]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default()
        };
        if !list("missing").is_empty() {
            bail!(
                "signing in at {}: no field for {} (the fields there: {})",
                login.url,
                list("missing"),
                list("available")
            );
        }
        if found["submitted"] != true {
            bail!(
                "signing in at {}: no submit button found; give `submit`, its text",
                login.url
            );
        }
        // A redirect, or a request and a change of route, takes a moment.
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let after = self
            .call(
                "Runtime.evaluate",
                json!({"expression": STILL_SIGNING_IN, "returnByValue": true}),
                Some(session),
            )
            .await?;
        let after = &after["result"]["value"];
        let same_page = after["path"].as_str().is_some_and(|path| {
            reqwest::Url::parse(&login.url)
                .map(|url| url.path() == path)
                .unwrap_or(false)
        });
        if after["password"] == true && same_page {
            let said = after["alert"].as_str().unwrap_or("").trim();
            bail!(
                "the sign-in at {} did not go through: the form is still there{}",
                login.url,
                if said.is_empty() {
                    String::new()
                } else {
                    format!(", saying \"{said}\"")
                }
            );
        }
        Ok(())
    }

    /// Errors the page logged or threw since it was loaded.
    fn console_errors(&self, session: &str) -> Vec<String> {
        let mut errors = Vec::new();
        for event in self.events.iter().filter(|e| e["sessionId"] == session) {
            let params = &event["params"];
            let text = match event["method"].as_str() {
                Some("Runtime.exceptionThrown") => params["exceptionDetails"]["exception"]
                    ["description"]
                    .as_str()
                    .or(params["exceptionDetails"]["text"].as_str())
                    .map(str::to_owned),
                Some("Runtime.consoleAPICalled") if params["type"] == "error" => params["args"]
                    .as_array()
                    .and_then(|args| args.first())
                    .and_then(|arg| arg["value"].as_str().or(arg["description"].as_str()))
                    .map(str::to_owned),
                Some("Log.entryAdded") if params["entry"]["level"] == "error" => params["entry"]
                    ["text"]
                    .as_str()
                    .map(|text| match params["entry"]["url"].as_str() {
                        Some(url) if !url.is_empty() => format!("{text} ({url})"),
                        _ => text.to_owned(),
                    }),
                _ => None,
            };
            if let Some(text) = text {
                let line: String = text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect();
                if !errors.contains(&line) {
                    errors.push(line);
                }
            }
        }
        errors.truncate(6);
        errors
    }
}

/// A dev server started for a preview and stopped after it.
struct Server {
    child: tokio::process::Child,
}

impl Server {
    async fn start(workspace: &Path, command: &str, url: &str, out_dir: &Path) -> Result<Self> {
        // Its output goes to a log, so a server that fails says why.
        let log_path = out_dir.join("server.log");
        let log = std::fs::File::create(&log_path)?;
        let mut process = tokio::process::Command::new("sh");
        process
            .arg("-c")
            .arg(command)
            .current_dir(workspace)
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true);
        #[cfg(unix)]
        process.process_group(0);
        let child = process
            .spawn()
            .with_context(|| format!("starting `{command}`"))?;
        let mut server = Self { child };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let failure = loop {
            if client.get(url).send().await.is_ok() {
                return Ok(server);
            }
            if let Ok(Some(status)) = server.child.try_wait() {
                break format!("`{command}` exited ({status}) before serving {url}");
            }
            if tokio::time::Instant::now() >= deadline {
                break format!("`{command}` did not start serving {url} within two minutes");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        server.stop().await;
        let output = std::fs::read_to_string(&log_path).unwrap_or_default();
        let lines: Vec<&str> = output.lines().collect();
        let tail = lines[lines.len().saturating_sub(20)..].join("\n");
        if tail.trim().is_empty() {
            bail!("{failure}");
        }
        bail!("{failure}. Its last output:\n{tail}")
    }

    /// Stop the server and everything it started (npm runs a child).
    async fn stop(mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGTERM,
            );
        }
        let _ = self.child.kill().await;
    }
}

/// The reports as the agent reads them: each width's problems, and where
/// its screenshot is.
pub fn report(target: &str, reports: &[WidthReport]) -> String {
    let mut out = format!("Preview of {target}\n");
    for r in reports {
        out.push_str(&format!(
            "\n{}px, page {}px tall: {}\n",
            r.width,
            r.page_height,
            match r.problems() {
                0 => "nothing found".to_owned(),
                1 => "1 problem".to_owned(),
                n => format!("{n} problems"),
            }
        ));
        let mut list = |label: &str, items: &[String]| {
            if !items.is_empty() {
                out.push_str(&format!("  {label}:\n"));
                for item in items {
                    out.push_str(&format!("    - {item}\n"));
                }
            }
        };
        if r.overflow_px > 1 {
            let wider = format!(
                "the page is {}px wider than the screen, caused by",
                r.overflow_px
            );
            list(&wider, &r.overflowing);
        }
        list("text below AA contrast", &r.low_contrast);
        list("links to nowhere", &r.dead_anchors);
        list("images without alt", &r.missing_alt);
        list("controls without a name", &r.unnamed_controls);
        list("touch targets under 44px", &r.small_targets);
        list("console errors", &r.console_errors);
        if !r.viewport_meta {
            out.push_str(
                "  no <meta name=\"viewport\">: a phone lays the page out at 980px and shrinks it\n",
            );
        }
        if r.header_scrolls_away {
            out.push_str(&format!(
                "  the top bar scrolls away on a page {} screens long: make it sticky \
                 (ui-part-header)\n",
                r.screens
            ));
        }
        if r.no_back_to_top {
            out.push_str(&format!(
                "  {} screens long, and halfway down there is no way back to the top: add a \
                 back-to-top control (ui-part-back-to-top)\n",
                r.screens
            ));
        }
        if let Some(blocks) = &r.repeated_blocks {
            out.push_str(&format!(
                "  the same block repeated down the page ({blocks}): give the lead one \
                 more room and its evidence, and set the rest as compact rows \
                 (ui-part-sections)\n"
            ));
        }
        if let Some(words) = r.long_headline {
            out.push_str(&format!(
                "  the headline has {words} words: a headline says one thing, in about \
                 twelve or fewer (ui-part-hero)\n"
            ));
        }
        if let Some(side) = &r.short_side {
            out.push_str(&format!(
                "  the side column stops short ({side}): a sidebar runs the full height of \
                 the window (sticky, 100dvh) (ui-part-sidebar)\n"
            ));
        }
        if let Some(buttons) = &r.repeated_primary {
            out.push_str(&format!(
                "  {buttons}: one filled primary action per view; actions in rows are \
                 quiet (ghost, outline or a menu) (ui-page-dashboard)\n"
            ));
        }
        if let Some(side) = &r.open_sidebar {
            out.push_str(&format!(
                "  a sidebar stays open at this width ({side}), squeezing the content: under \
                 1024px it becomes a drawer behind a labelled Menu button in a top bar \
                 (ui-part-sidebar, ui-layout section 5)\n"
            ));
        }
        if let Some(gaps) = &r.floating_content {
            out.push_str(&format!(
                "  the content floats centred beside the sidebar ({gaps}): anchor it at the \
                 sidebar's edge plus the page padding, with no mx-auto on the page column \
                 (ui-layout section 2b)\n"
            ));
        }
        if let Some(background) = &r.grey_background {
            out.push_str(&format!(
                "  the page background is {background}: a dark theme sits at 3 to 8% \
                 lightness, a light one at 93% or more (the `ui` skill, neutrals)\n"
            ));
        }
        if r.h1_count != 1 {
            out.push_str(&format!("  {} h1 elements; a page has one\n", r.h1_count));
        }
        if r.small_text > 0 {
            out.push_str(&format!("  {} text elements under 12px\n", r.small_text));
        }
        if let Some(shot) = &r.screenshot {
            out.push_str(&format!("  screenshot: {}\n", shot.display()));
        }
    }
    out.push_str(
        "\nThe measurements are facts about the rendered page; fix what they show. The \
         screenshots are for the user to look at, and for you if you can read images.",
    );
    out
}

/// Run on the sign-in page: fill each field and press submit. A field is
/// found by its name or id, then its type (for `password` and `email`), its
/// label, its placeholder, or its autocomplete hint; values are set the way
/// typing sets them, so frameworks that track input see them.
const LOGIN_SCRIPT: &str = r#"(async () => {
  const fields = __FIELDS__;
  const submitText = __SUBMIT__;
  const shown = el => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  const inputs = [...document.querySelectorAll('input:not([type=hidden]):not([type=submit]):not([type=button]):not([type=checkbox]):not([type=radio]), textarea')].filter(shown);
  const norm = s => (s || '').toLowerCase().replace(/\s+/g, ' ').trim();
  const labelOf = el => norm([...(el.labels || [])].map(l => l.textContent).join(' ') + ' ' + (el.getAttribute('aria-label') || ''));
  const find = key => {
    const k = norm(key);
    return inputs.find(el => norm(el.name) === k || norm(el.id) === k)
      || inputs.find(el => (k === 'password' || k === 'email') && norm(el.type) === k)
      || inputs.find(el => labelOf(el).includes(k))
      || inputs.find(el => norm(el.placeholder).includes(k))
      || inputs.find(el => norm(el.getAttribute('autocomplete')).includes(k));
  };
  const setValue = (el, value) => {
    const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
    el.focus();
    setter.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const available = inputs.map(el => el.name || el.id || el.placeholder || el.type).slice(0, 8);
  const missing = [];
  let form = null;
  for (const [key, value] of Object.entries(fields)) {
    const el = find(key);
    if (!el) { missing.push(key); continue; }
    setValue(el, value);
    form = form || el.form;
  }
  if (missing.length) return { missing, available, submitted: false };
  await new Promise(done => setTimeout(done, 150));
  const buttons = [...(form || document).querySelectorAll('button, input[type=submit]')].filter(shown);
  let button = submitText ? buttons.find(b => norm(b.textContent || b.value).includes(norm(submitText))) : null;
  button = button || buttons.find(b => (b.getAttribute('type') || 'submit') === 'submit');
  if (button) { button.click(); return { missing, available, submitted: true }; }
  if (form) { form.requestSubmit ? form.requestSubmit() : form.submit(); return { missing, available, submitted: true }; }
  return { missing, available, submitted: false };
})()"#;

/// Run after submitting: whether the sign-in form is still on screen, and
/// what the page says about it.
const STILL_SIGNING_IN: &str = r#"(() => {
  const password = [...document.querySelectorAll('input[type=password]')].some(el => {
    const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0;
  });
  const alert = [...document.querySelectorAll('[role=alert], [aria-live=assertive]')]
    .map(el => el.textContent.trim()).filter(Boolean).join(' ').slice(0, 200);
  return { password, alert, path: location.pathname };
})()"#;

/// Run in the page: what a person would run into, as plain data.
const CHECK_SCRIPT: &str = r#"(async () => {
  try { await document.fonts.ready; } catch (e) {}
  // The width asked for, not innerWidth: a phone zooms out to fit a page
  // wider than itself, and innerWidth grows to match the overflow.
  const W = __WIDTH__;
  const out = {};
  const describe = el => {
    let s = el.tagName.toLowerCase();
    if (el.id) s += '#' + el.id;
    else if (el.classList && el.classList.length) s += '.' + [...el.classList].slice(0, 2).join('.');
    return s;
  };
  const visible = el => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none';
  };
  out.height = window.innerWidth > W + 1
    ? Math.ceil(document.documentElement.getBoundingClientRect().height)
    : document.documentElement.scrollHeight;
  out.h1 = document.querySelectorAll('h1').length;
  out.viewport_meta = !!document.querySelector('meta[name=viewport]');
  out.overflow_px = document.documentElement.scrollWidth - W;
  out.overflowing = [];
  const clipped = el => {
    for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
      if (getComputedStyle(a).overflowX !== 'visible') return true;
    }
    return false;
  };
  if (out.overflow_px > 1) {
    for (const el of document.querySelectorAll('body *')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.right > W + 1 && !clipped(el)) {
        const parent = el.parentElement;
        if (parent && parent.getBoundingClientRect().right > W + 1 && parent !== document.body) continue;
        out.overflowing.push(describe(el) + ' reaches ' + Math.round(r.right) + 'px');
        if (out.overflowing.length >= 6) break;
      }
    }
  }
  out.missing_alt = [...document.images].filter(i => !i.hasAttribute('alt'))
    .slice(0, 6).map(i => i.getAttribute('src') || describe(i));
  out.dead_anchors = [...document.querySelectorAll('a[href]')].filter(a => {
    const h = a.getAttribute('href');
    if (h === '#' || h === '') return true;
    // `#top` is the top of the document by definition, with or without an
    // element of that id.
    if (h.toLowerCase() === '#top') return false;
    if (h.startsWith('#')) return !document.getElementById(decodeURIComponent(h.slice(1)));
    return false;
  }).slice(0, 6).map(a => (a.textContent.trim().slice(0, 30) || describe(a)) + ' -> ' + a.getAttribute('href'));
  out.unnamed_controls = [...document.querySelectorAll('button, a[href], [role=button], input:not([type=hidden]), select, textarea')]
    .filter(visible).filter(el => {
      const name = (el.getAttribute('aria-label') || el.getAttribute('title') || el.textContent || '').trim();
      if (name) return false;
      if (el.querySelector('img[alt]:not([alt=""])')) return false;
      if (el.labels && el.labels.length) return false;
      if (el.getAttribute('aria-labelledby')) return false;
      if (el.getAttribute('placeholder')) return false;
      return true;
    }).slice(0, 6).map(describe);
  const parse = c => {
    const m = c && c.match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const p = m[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 };
  };
  const lum = c => {
    const f = v => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
  };
  const background = el => {
    for (let e = el; e; e = e.parentElement) {
      const cs = getComputedStyle(e);
      if (cs.backgroundImage && cs.backgroundImage !== 'none') return null;
      const c = parse(cs.backgroundColor);
      if (c && c.a > 0.95) return c;
    }
    return { r: 255, g: 255, b: 255, a: 1 };
  };
  const seen = new Set();
  const low = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  let checked = 0;
  while (walker.nextNode() && checked < 800) {
    const node = walker.currentNode;
    if (!node.textContent.trim()) continue;
    const el = node.parentElement;
    if (!el || seen.has(el) || !visible(el)) continue;
    seen.add(el);
    checked++;
    const cs = getComputedStyle(el);
    const fg = parse(cs.color);
    const bg = background(el);
    if (!fg || !bg) continue;
    const a = fg.a;
    const shown = { r: fg.r * a + bg.r * (1 - a), g: fg.g * a + bg.g * (1 - a), b: fg.b * a + bg.b * (1 - a) };
    const l1 = lum(shown), l2 = lum(bg);
    const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    const size = parseFloat(cs.fontSize);
    const bold = parseInt(cs.fontWeight, 10) >= 700;
    const need = (size >= 24 || (size >= 18.66 && bold)) ? 3 : 4.5;
    if (ratio < need) low.push({ ratio, text: describe(el) + ' "' + node.textContent.trim().slice(0, 40) + '" ' + ratio.toFixed(2) + ':1, needs ' + need });
  }
  low.sort((x, y) => x.ratio - y.ratio);
  out.low_contrast = low.slice(0, 6).map(x => x.text);
  out.small_text = [...seen].filter(el => parseFloat(getComputedStyle(el).fontSize) < 12).length;
  // A link inside a sentence is exempt, as WCAG exempts it.
  const inText = el => {
    if (el.tagName !== 'A') return false;
    if (el.closest('p')) return true;
    const parent = el.parentElement;
    return !!parent && getComputedStyle(el).display === 'inline'
      && parent.textContent.trim().length > el.textContent.trim().length + 20;
  };
  // Hidden for everyone but screen readers until focused, like a skip
  // link: a 1px box, or a clipped one. Not a touch target at rest.
  const readerOnly = el => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return (r.width <= 2 && r.height <= 2)
      || (cs.position === 'absolute' && cs.clip && cs.clip !== 'auto')
      || cs.clipPath === 'inset(50%)';
  };
  out.small_targets = [];
  if (W < 600) {
    out.small_targets = [...document.querySelectorAll('a[href], button, [role=button], input:not([type=hidden]), select, summary')]
      .filter(visible)
      .filter(el => !inText(el) && !readerOnly(el) && !(el.labels && el.labels.length && (el.type === 'checkbox' || el.type === 'radio')))
      .filter(el => { const r = el.getBoundingClientRect(); return r.height < 44 || r.width < 44; })
      .slice(0, 6).map(el => {
        const r = el.getBoundingClientRect();
        return describe(el) + ' ' + Math.round(r.width) + 'x' + Math.round(r.height) + 'px';
      });
  }
  // The same tall block stacked four or more times: one module repeated
  // down the page. Blocks are compared by their tag, classes and the tags
  // of their children, which is what a template repeats.
  out.repeated_blocks = null;
  let worst = 0;
  const shape = el => el.tagName + '.' + [...el.classList].sort().join('.') + '>'
    + [...el.children].map(c => c.tagName).join(',');
  for (const parent of document.querySelectorAll('body *')) {
    // Table rows are meant to repeat, even when a phone lays them out as
    // cards.
    if (parent.tagName === 'TBODY' || parent.tagName === 'TABLE' || parent.getAttribute('role') === 'rowgroup') continue;
    const kids = [...parent.children].filter(el => {
      const r = el.getBoundingClientRect();
      return r.height > 120 && r.width > W * 0.4;
    });
    let run = 1;
    for (let i = 1; i <= kids.length; i++) {
      if (i < kids.length && shape(kids[i]) === shape(kids[i - 1])) { run++; continue; }
      if (run >= 4 && run > worst) {
        worst = run;
        const el = kids[i - 1];
        const kind = el.tagName.toLowerCase()
          + (el.classList.length ? '.' + [...el.classList].slice(0, 2).join('.') : '');
        out.repeated_blocks = kind + ' ×' + run;
      }
      run = 1;
    }
  }
  // A side column with its own background, looked at again once the page
  // is scrolled: one that stops short leaves the window's side empty.
  out.short_side = null;
  const side = W < 1024 ? null : [...document.querySelectorAll('aside, nav, [class*=sidebar]')].find(el => {
    const r = el.getBoundingClientRect();
    const bg = parse(getComputedStyle(el).backgroundColor);
    return r.left <= 8 && r.top <= 8 && r.width >= 120 && r.width <= W * 0.4 && bg && bg.a >= 0.5;
  });
  // The same saturated filled button, again and again.
  out.repeated_primary = null;
  const fills = {};
  for (const el of document.querySelectorAll('button, a[href], [role=button]')) {
    if (!visible(el) || readerOnly(el)) continue;
    const c = parse(getComputedStyle(el).backgroundColor);
    if (!c || c.a < 0.9) continue;
    const max = Math.max(c.r, c.g, c.b), min = Math.min(c.r, c.g, c.b);
    if (max === 0 || (max - min) / max < 0.25) continue;
    const key = '#' + [c.r, c.g, c.b].map(v => Math.round(v).toString(16).padStart(2, '0')).join('');
    fills[key] = (fills[key] || 0) + 1;
  }
  for (const [colour, count] of Object.entries(fills)) {
    if (count >= 5) out.repeated_primary = count + ' filled buttons in ' + colour;
  }
  // The page's background, as the reader sees it behind the content.
  out.grey_background = null;
  const pageBg = [document.body, document.documentElement]
    .map(el => el && parse(getComputedStyle(el).backgroundColor))
    .find(c => c && c.a > 0.95);
  if (pageBg) {
    const light = (Math.max(pageBg.r, pageBg.g, pageBg.b) + Math.min(pageBg.r, pageBg.g, pageBg.b)) / 2 / 255 * 100;
    if (light > 9 && light < 92) {
      const hex = '#' + [pageBg.r, pageBg.g, pageBg.b].map(v => Math.round(v).toString(16).padStart(2, '0')).join('');
      out.grey_background = hex + ', ' + Math.round(light) + '% light';
    }
  }
  // An application shell: a tall column pinned to the left edge.
  const shellSide = [...document.querySelectorAll('aside, nav, [class*=sidebar], [role=navigation]')].find(el => {
    if (!visible(el)) return false;
    const r = el.getBoundingClientRect();
    return r.left <= 2 && r.top <= 80 && r.height >= window.innerHeight * 0.7
      && r.width >= 56 && r.width <= Math.max(360, W * 0.45);
  });
  const kindOf = el => el.tagName.toLowerCase() + (el.classList.length ? '.' + [...el.classList].slice(0, 2).join('.') : '');
  // Still beside the content on a phone or a tablet: a drawer would be shut.
  out.open_sidebar = null;
  if (shellSide && W < 1024) {
    const r = shellSide.getBoundingClientRect();
    if (r.width >= 120) out.open_sidebar = kindOf(shellSide) + ' takes ' + Math.round(r.width) + ' of ' + W + 'px';
  }
  // On a wide monitor, whether the content stays beside the sidebar or
  // floats centred in what is left, with an empty band on each side. The
  // page is laid out as if 2400px wide for a moment to see it.
  out.floating_content = null;
  const content = shellSide && (document.querySelector('main') || shellSide.nextElementSibling);
  if (shellSide && content && W >= 1200) {
    const root = document.documentElement;
    const was = root.style.width;
    root.style.width = '2400px';
    await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
    const side = shellSide.getBoundingClientRect();
    let left = Infinity, right = -Infinity, counted = 0;
    for (const el of content.querySelectorAll('*')) {
      if (counted > 1500) break;
      if (el.children.length && !/^(TABLE|INPUT|BUTTON|SELECT|TEXTAREA|IMG|SVG|CANVAS|VIDEO)$/i.test(el.tagName)) continue;
      const r = el.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0 || getComputedStyle(el).position === 'fixed') continue;
      counted++;
      left = Math.min(left, r.left);
      right = Math.max(right, r.right);
    }
    const gapLeft = left - side.right;
    const gapRight = 2400 - right;
    if (counted && gapLeft > 96 && gapRight > 96 && Math.abs(gapLeft - gapRight) < 64) {
      out.floating_content = Math.round(gapLeft) + 'px from the sidebar and ' + Math.round(gapRight)
        + 'px from the right edge on a 2400px screen';
    }
    root.style.width = was;
    await new Promise(done => requestAnimationFrame(() => done()));
  }
  const h1 = document.querySelector('h1');
  const words = h1 ? h1.textContent.trim().split(/\s+/).filter(Boolean).length : 0;
  out.long_headline = words > 14 ? words : null;
  // A long page, scrolled: whether the top bar stays in view, and whether
  // there is a way back to the top from halfway down. Measured by scrolling
  // rather than read from the CSS, so a sticky bar that a wrapper's overflow
  // stops from sticking counts as scrolling away.
  const root = document.documentElement;
  const screen = window.innerHeight;
  out.screens = Math.round(root.scrollHeight / screen * 10) / 10;
  out.header_scrolls_away = false;
  out.no_back_to_top = false;
  if (out.screens > 1.5) {
    const header = [...document.querySelectorAll('header, [role=banner], nav')].find(el => {
      const r = el.getBoundingClientRect();
      return r.height > 0 && r.top + window.scrollY < 80 && r.width > W * 0.5 && r.height < screen / 2;
    });
    const behaviour = root.style.scrollBehavior;
    root.style.scrollBehavior = 'auto';
    const settle = () => new Promise(done => setTimeout(done, 400));
    const bottom = root.scrollHeight - screen;
    window.scrollTo(0, Math.min(Math.max(screen, bottom / 2), bottom));
    await settle();
    if (header) {
      const r = header.getBoundingClientRect();
      out.header_scrolls_away = r.bottom <= 1 || r.top >= screen;
    }
    if (side) {
      const r = side.getBoundingClientRect();
      if (r.bottom < screen - 2) {
        const kind = side.tagName.toLowerCase() + (side.classList.length ? '.' + [...side.classList].slice(0, 2).join('.') : '');
        out.short_side = kind + ' covers ' + Math.max(0, Math.round(r.bottom)) + ' of ' + screen + 'px once scrolled';
      }
    }
    if (out.screens > 3) {
      const named = /\bto\s+(the\s+)?top\b|^\s*top\s*$|\bke atas\b/i;
      const atTop = el => el === document.body || el === root
        || (el.getBoundingClientRect().top + window.scrollY < 50 && !(header && header.contains(el)));
      out.no_back_to_top = ![...document.querySelectorAll('a[href], button, [role=button]')].some(el => {
        const r = el.getBoundingClientRect();
        if (!visible(el) || r.bottom <= 0 || r.top >= screen) return false;
        if (parseFloat(getComputedStyle(el).opacity) < 0.1) return false;
        const href = el.getAttribute('href') || '';
        if (href.toLowerCase() === '#top') return true;
        if (href.length > 1 && href.startsWith('#')) {
          const target = document.getElementById(decodeURIComponent(href.slice(1)));
          if (target && atTop(target)) return true;
        }
        const name = [el.getAttribute('aria-label'), el.getAttribute('title'), el.textContent].join(' ');
        return named.test(name.trim());
      });
    }
    window.scrollTo(0, 0);
    await settle();
    root.style.scrollBehavior = behaviour;
  }
  return out;
})()"#;
