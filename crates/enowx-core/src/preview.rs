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
use serde::Serialize;
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
    let result = look(&chrome, &url, out_dir).await;
    if let Some(server) = server {
        server.stop().await;
    }
    result
}

async fn look(chrome: &Path, url: &str, out_dir: &Path) -> Result<Vec<WidthReport>> {
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
        self.wait_for("Page.loadEventFired", session, Duration::from_secs(30))
            .await;
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
  out.small_targets = [];
  if (W < 600) {
    out.small_targets = [...document.querySelectorAll('a[href], button, [role=button], input:not([type=hidden]), select, summary')]
      .filter(visible)
      .filter(el => !inText(el) && !(el.labels && el.labels.length && (el.type === 'checkbox' || el.type === 'radio')))
      .filter(el => { const r = el.getBoundingClientRect(); return r.height < 44 || r.width < 44; })
      .slice(0, 6).map(el => {
        const r = el.getBoundingClientRect();
        return describe(el) + ' ' + Math.round(r.width) + 'x' + Math.round(r.height) + 'px';
      });
  }
  return out;
})()"#;
