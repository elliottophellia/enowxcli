//! Looking at a page in a real browser.
//!
//! Every interface an agent built was reported "not checked in a browser;
//! checked by reading the CSS". This opens the page in headless Chrome at a
//! phone, a tablet and a wide width, measures what a person would see
//! (horizontal overflow, text contrast, links to nowhere, images without alt,
//! controls without a name, small touch targets, console errors) and saves a
//! screenshot of each width. It talks to Chrome over the DevTools protocol,
//! and starts and stops a dev server when the page needs one.
//!
//! Asked to, it also watches how the page moves (`MotionReport`): what
//! animates on load and on scroll, loops that never stop, animation that
//! costs layout, content that never appears, and the page again with reduced
//! motion. An agent reads text, not images, so that timeline is how it sees
//! motion.

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
    /// Drawings and icons whose main colour is below 3:1 against what is
    /// behind them, as `svg.ship 540x160 drawn in #000000 on #0f1419,
    /// 1.08:1, needs 3:1`.
    pub low_contrast_graphics: Vec<String>,
    /// The theme this report was taken in, when it is the page's second:
    /// `dark` or `light`.
    pub theme: Option<String>,
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
            + self.low_contrast_graphics.len()
            + usize::from(self.open_sidebar.is_some())
    }

    /// Keep only what a theme changes: colour and contrast. The layout was
    /// measured in the first theme already.
    fn keep_theme_findings(&mut self) {
        let keep = WidthReport {
            width: self.width,
            page_height: self.page_height,
            low_contrast: std::mem::take(&mut self.low_contrast),
            low_contrast_graphics: std::mem::take(&mut self.low_contrast_graphics),
            grey_background: self.grey_background.take(),
            console_errors: std::mem::take(&mut self.console_errors),
            theme: self.theme.take(),
            screenshot: self.screenshot.take(),
            h1_count: 1,
            viewport_meta: true,
            ..WidthReport::default()
        };
        *self = keep;
    }
}

/// How the page moves, watched at 1440px: once as it is, once with reduced
/// motion. Each list is short and already worded for the agent.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MotionReport {
    pub width: u32,
    /// Animations on load in the order they start, as `at 120ms for 900ms:
    /// h1 span.hero-line, opacity, moves 18px`.
    pub on_load: Vec<String>,
    /// How many played on load, and when the last of them ends, in ms from
    /// the start of loading.
    pub load_count: usize,
    pub load_ends_ms: u64,
    /// How many animations scrolling through the page started, and in how
    /// many blocks (what one reveal starts, or one burst).
    pub on_scroll: usize,
    pub scroll_blocks: usize,
    /// The block whose animations run longest, as `div.steps: 18
    /// animations over 2.9s`.
    pub longest_sequence: Option<String>,
    /// CSS animations that repeat forever, as `span.caret: blink 1s`.
    pub endless: Vec<String>,
    /// Script loops: requestAnimationFrame and fast timers, and whether they
    /// stop when their part of the page is scrolled away.
    pub loops: Vec<String>,
    /// Loops that keep running with the page scrolled away from them.
    pub never_stops: usize,
    /// Animated properties that cost layout or heavy paint.
    pub layout_animated: Vec<String>,
    /// Rules with `transition: all` (or a transition with no property).
    pub transition_all: Vec<String>,
    pub transition_all_count: usize,
    /// Elements whose inline style a script rewrites many times a second.
    pub js_animated: Vec<String>,
    /// Frames of 100ms or more while scrolling, as `3, the longest 180ms
    /// (main.js)`.
    pub long_frames: Option<String>,
    /// Layout shift while loading and revealing, when over 0.05.
    pub layout_shift: Option<String>,
    /// Text still invisible after scrolling through the whole page.
    pub hidden: Vec<String>,
    /// Scroll, wheel, touch and pointer-move listeners.
    pub listeners: Vec<String>,
    /// Wheel or touch listeners that can block scrolling.
    pub blocking_listeners: usize,
    /// IntersectionObservers the page made, and how many elements they watch.
    pub observers: Option<String>,
    /// With reduced motion: what still moves, what still loops, what stays
    /// hidden, and whether smooth scrolling is left on.
    pub reduced_moving: Vec<String>,
    pub reduced_loops: Vec<String>,
    pub reduced_hidden: Vec<String>,
    pub reduced_smooth_scroll: bool,
    /// Screenshots of the first seconds and of the first reveal, for the
    /// user.
    pub frames: Vec<PathBuf>,
    /// Set when the page could not be watched (its scripts never ran).
    pub note: Option<String>,
}

impl MotionReport {
    /// How many problems the motion showed. Endless CSS animations, script
    /// loops that stop off screen, listeners and observers are facts to
    /// judge, not problems in themselves.
    pub fn problems(&self) -> usize {
        self.hidden.len()
            + self.layout_animated.len()
            + usize::from(self.transition_all_count > 0)
            + self.never_stops
            + usize::from(self.long_frames.is_some())
            + usize::from(self.layout_shift.is_some())
            + self.blocking_listeners
            + self.reduced_moving.len()
            + self.reduced_loops.len()
            + self.reduced_hidden.len()
            + usize::from(self.reduced_smooth_scroll)
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
    Ok(
        preview_with(workspace, target, start, login, false, out_dir)
            .await?
            .0,
    )
}

/// `preview_signed_in`, and with `motion` also how the page moves.
pub async fn preview_with(
    workspace: &Path,
    target: Target,
    start: Option<&str>,
    login: Option<&Login>,
    motion: bool,
    out_dir: &Path,
) -> Result<(Vec<WidthReport>, Option<MotionReport>)> {
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
    let result = look(&chrome, &url, login, motion, out_dir).await;
    if let Some(server) = server {
        server.stop().await;
    }
    result
}

/// One headless Chrome for every look, shared by the agents of the process.
///
/// Each look used to start its own browser and profile, so an agent that
/// looked after every edit, or four page agents at once, kept several
/// Chromes of a few hundred megabytes each alive. Now a look opens a
/// throwaway browser context (its own cookies and storage) in the one
/// browser, at most `LOOKS_AT_ONCE` at a time, and the browser closes itself
/// after `IDLE` without a look.
struct Browser {
    child: tokio::process::Child,
    endpoint: String,
    profile: PathBuf,
    looking: usize,
    last_used: std::time::Instant,
}

const LOOKS_AT_ONCE: usize = 2;
const IDLE: Duration = Duration::from_secs(90);

static BROWSER: tokio::sync::Mutex<Option<Browser>> = tokio::sync::Mutex::const_new(None);
static LOOKS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(LOOKS_AT_ONCE);

fn pid_file() -> PathBuf {
    crate::config::home_dir().join("chrome.pid")
}

/// A browser left running by an enx that exited without closing it: stop it
/// when its command line says it is ours.
async fn stop_leftover() {
    let Ok(text) = std::fs::read_to_string(pid_file()) else {
        return;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return;
    };
    let command = tokio::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .await;
    if let Ok(out) = command {
        if String::from_utf8_lossy(&out.stdout).contains("enx-chrome-") {
            let _ = tokio::process::Command::new("kill")
                .args(["-TERM", &format!("-{pid}")])
                .status()
                .await;
        }
    }
    let _ = std::fs::remove_file(pid_file());
}

async fn launch(chrome: &Path) -> Result<Browser> {
    stop_leftover().await;
    let profile = std::env::temp_dir().join(format!("enx-chrome-{}", uuid::Uuid::new_v4()));
    // Chrome runs under a small shell that watches enx: when enx is gone,
    // however it ended, the shell stops Chrome. A browser kept for the whole
    // process would otherwise outlive a crash or a test run. The shell leads
    // its own process group, so closing it stops Chrome and its helpers.
    const WATCH: &str = "\"$0\" \"$@\" & c=$!; p=$PPID; \
        while kill -0 $p 2>/dev/null && kill -0 $c 2>/dev/null; do sleep 2; done; \
        kill $c 2>/dev/null; wait $c 2>/dev/null";
    let mut child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(WATCH)
        .arg(chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--hide-scrollbars",
            "--mute-audio",
            "--disable-extensions",
            "--disable-background-networking",
            "--remote-debugging-port=0",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .process_group(0)
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
    if let Some(pid) = child.id() {
        let _ = std::fs::write(pid_file(), pid.to_string());
    }
    // Close the browser once nobody has looked for a while.
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let mut slot = BROWSER.lock().await;
            let Some(browser) = slot.as_mut() else {
                return;
            };
            if browser.looking == 0 && browser.last_used.elapsed() >= IDLE {
                if let Some(browser) = slot.take() {
                    close(browser).await;
                }
                return;
            }
        }
    });
    Ok(Browser {
        child,
        endpoint,
        profile,
        looking: 0,
        last_used: std::time::Instant::now(),
    })
}

async fn close(mut browser: Browser) {
    // The whole group: the watching shell, Chrome and its helpers.
    if let Some(group) = browser.child.id() {
        let _ = tokio::process::Command::new("kill")
            .args(["-TERM", &format!("-{group}")])
            .status()
            .await;
    }
    let _ = browser.child.kill().await;
    let _ = std::fs::remove_dir_all(&browser.profile);
    let _ = std::fs::remove_file(pid_file());
}

/// Close the shared browser now: at exit, so no Chrome outlives enx.
pub async fn shutdown() {
    if let Some(browser) = BROWSER.lock().await.take() {
        close(browser).await;
    }
}

/// The endpoint of the running browser, starting one when there is none or
/// the last one died, counted as in use until `done_looking`.
async fn borrow(chrome: &Path) -> Result<String> {
    let mut slot = BROWSER.lock().await;
    let alive = slot
        .as_mut()
        .is_some_and(|browser| matches!(browser.child.try_wait(), Ok(None)));
    if !alive {
        if let Some(dead) = slot.take() {
            close(dead).await;
        }
        *slot = Some(launch(chrome).await?);
    }
    let browser = slot.as_mut().context("a browser")?;
    browser.looking += 1;
    browser.last_used = std::time::Instant::now();
    Ok(browser.endpoint.clone())
}

async fn done_looking() {
    if let Some(browser) = BROWSER.lock().await.as_mut() {
        browser.looking = browser.looking.saturating_sub(1);
        browser.last_used = std::time::Instant::now();
    }
}

async fn look(
    chrome: &Path,
    url: &str,
    login: Option<&Login>,
    motion: bool,
    out_dir: &Path,
) -> Result<(Vec<WidthReport>, Option<MotionReport>)> {
    let _turn = LOOKS.acquire().await.context("looking")?;
    let endpoint = borrow(chrome).await?;
    let outcome = look_in(&endpoint, url, login, motion, out_dir).await;
    done_looking().await;
    outcome
}

async fn look_in(
    endpoint: &str,
    url: &str,
    login: Option<&Login>,
    motion: bool,
    out_dir: &Path,
) -> Result<(Vec<WidthReport>, Option<MotionReport>)> {
    let mut cdp = Cdp::connect(endpoint).await?;
    // A context of its own: its cookies, storage and sign-in end with it.
    let context = cdp
        .call("Target.createBrowserContext", json!({}), None)
        .await?["browserContextId"]
        .as_str()
        .context("a browser context")?
        .to_owned();
    let outcome = async {
        let target = cdp
            .call(
                "Target.createTarget",
                json!({"url": "about:blank", "browserContextId": context}),
                None,
            )
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
        // The first look is in the light system setting, whatever this
        // machine's is, so a page reports the same everywhere; the other
        // theme gets its own look at the end.
        cdp.call(
            "Emulation.setEmulatedMedia",
            json!({"features": [{"name": "prefers-color-scheme", "value": "light"}]}),
            Some(&session),
        )
        .await?;
        if let Some(login) = login {
            cdp.sign_in(&session, login).await?;
        }
        let mut reports = Vec::new();
        for width in WIDTHS {
            reports.push(cdp.look_at(&session, url, width, out_dir).await?);
        }
        if let Some(other) = cdp.look_in_other_theme(&session, url, out_dir).await? {
            reports.push(other);
        }
        let moved = if motion {
            Some(cdp.watch_motion(&session, url, out_dir).await?)
        } else {
            None
        };
        Ok::<_, anyhow::Error>((reports, moved))
    }
    .await;
    let _ = cdp
        .call(
            "Target.disposeBrowserContext",
            json!({"browserContextId": context}),
            None,
        )
        .await;
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
        self.open(session, url, width).await?;
        self.measure(session, width, out_dir, &format!("{width}.png"))
            .await
    }

    /// The page again at 1440px in its other theme, when it has one: the
    /// system setting flipped, and the usual class and attribute switches
    /// flipped too when the setting alone changes nothing. None for a page
    /// with one theme.
    async fn look_in_other_theme(
        &mut self,
        session: &str,
        url: &str,
        out_dir: &Path,
    ) -> Result<Option<WidthReport>> {
        let width = 1440;
        self.open(session, url, width).await?;
        let Some(before) = self.page_lightness(session).await? else {
            return Ok(None);
        };
        let other = if before >= 50.0 { "dark" } else { "light" };
        self.call(
            "Emulation.setEmulatedMedia",
            json!({"features": [{"name": "prefers-color-scheme", "value": other}]}),
            Some(session),
        )
        .await?;
        self.open(session, url, width).await?;
        let mut after = self.page_lightness(session).await?.unwrap_or(before);
        if (after - before).abs() < 20.0 {
            self.call(
                "Runtime.evaluate",
                json!({"expression": FLIP_THEME.replace("__THEME__", other),
                       "returnByValue": true, "awaitPromise": true}),
                Some(session),
            )
            .await?;
            after = self.page_lightness(session).await?.unwrap_or(before);
        }
        let report = if (after - before).abs() < 20.0 {
            None
        } else {
            let mut report = self
                .measure(session, width, out_dir, &format!("{width}-{other}.png"))
                .await?;
            report.theme = Some(other.to_owned());
            report.keep_theme_findings();
            Some(report)
        };
        let _ = self
            .call(
                "Emulation.setEmulatedMedia",
                json!({"features": [{"name": "prefers-color-scheme", "value": "light"}]}),
                Some(session),
            )
            .await;
        Ok(report)
    }

    /// The lightness of the page's background, 0 to 100, or None when no
    /// solid background can be found.
    async fn page_lightness(&mut self, session: &str) -> Result<Option<f64>> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({"expression": PAGE_LIGHTNESS, "returnByValue": true}),
                Some(session),
            )
            .await?;
        Ok(result["result"]["value"].as_f64())
    }

    /// Load `url` at `width` and wait for it to settle.
    async fn open(&mut self, session: &str, url: &str, width: u32) -> Result<()> {
        self.navigate(session, url, width).await?;
        // Late scripts and web fonts settle before anything is measured.
        tokio::time::sleep(Duration::from_millis(600)).await;
        Ok(())
    }

    /// Load `url` at `width`, returning as soon as it has loaded so its
    /// first moments can be watched.
    async fn navigate(&mut self, session: &str, url: &str, width: u32) -> Result<()> {
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
        Ok(())
    }

    /// Run `expression` in the page and return its value.
    async fn evaluate(&mut self, session: &str, expression: &str) -> Result<Value> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({"expression": expression, "returnByValue": true, "awaitPromise": true}),
                Some(session),
            )
            .await?;
        Ok(result["result"]["value"].clone())
    }

    /// The media every look emulates: the light setting, and `motion` for
    /// reduced motion (`reduce` or `no-preference`).
    async fn emulate_motion(&mut self, session: &str, motion: &str) -> Result<()> {
        self.call(
            "Emulation.setEmulatedMedia",
            json!({"features": [
                {"name": "prefers-color-scheme", "value": "light"},
                {"name": "prefers-reduced-motion", "value": motion},
            ]}),
            Some(session),
        )
        .await?;
        Ok(())
    }

    /// Save what is on screen now as `name` in `out_dir`.
    async fn frame(&mut self, session: &str, out_dir: &Path, name: &str) -> Result<PathBuf> {
        let capture = self
            .call(
                "Page.captureScreenshot",
                json!({"format": "png"}),
                Some(session),
            )
            .await?;
        let data = capture["data"].as_str().context("a screenshot")?;
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
        let path = out_dir.join(name);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    /// Watch how the page moves at 1440px: as it is, then with reduced
    /// motion. Hooks installed before the page's own scripts record every
    /// animation as it starts, the loops and the listeners.
    async fn watch_motion(
        &mut self,
        session: &str,
        url: &str,
        out_dir: &Path,
    ) -> Result<MotionReport> {
        let added = self
            .call(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({"source": MOTION_HOOKS}),
                Some(session),
            )
            .await?;
        let hooks = added["identifier"].as_str().unwrap_or_default().to_owned();
        let watched = self.watch_motion_twice(session, url, out_dir).await;
        // Whatever happened, the page is left as the other looks expect it.
        let _ = self.emulate_motion(session, "no-preference").await;
        if !hooks.is_empty() {
            let _ = self
                .call(
                    "Page.removeScriptToEvaluateOnNewDocument",
                    json!({"identifier": hooks}),
                    Some(session),
                )
                .await;
        }
        watched
    }

    async fn watch_motion_twice(
        &mut self,
        session: &str,
        url: &str,
        out_dir: &Path,
    ) -> Result<MotionReport> {
        let width = 1440;
        let mut report = MotionReport {
            width,
            ..MotionReport::default()
        };

        // As it is: the first seconds photographed, a second at rest, then a
        // visitor's scroll to the bottom and a second there.
        self.emulate_motion(session, "no-preference").await?;
        self.navigate(session, url, width).await?;
        let loaded = tokio::time::Instant::now();
        for at in [0u64, 300, 700, 1300, 2500] {
            tokio::time::sleep_until(loaded + Duration::from_millis(at)).await;
            if let Ok(path) = self
                .frame(session, out_dir, &format!("motion-{at}ms.png"))
                .await
            {
                report.frames.push(path);
            }
        }
        let rest = self.evaluate(session, MOTION_AT_REST).await?;
        if rest.is_null() {
            report.note =
                Some("the page's scripts did not run, so its motion could not be watched".into());
            return Ok(report);
        }
        let mut frames = Vec::new();
        self.scroll_through(
            session,
            Duration::from_millis(260),
            Some(out_dir),
            &mut frames,
        )
        .await?;
        report.frames.extend(frames);
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let found = self.evaluate(session, MOTION_REPORT).await?;
        read_motion(&mut report, &rest, &found);

        // With reduced motion.
        self.emulate_motion(session, "reduce").await?;
        self.navigate(session, url, width).await?;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let rest = self.evaluate(session, MOTION_AT_REST).await?;
        self.scroll_through(session, Duration::from_millis(150), None, &mut Vec::new())
            .await?;
        tokio::time::sleep(Duration::from_millis(800)).await;
        let found = self.evaluate(session, MOTION_REPORT).await?;
        read_reduced(&mut report, &rest, &found);
        Ok(report)
    }

    /// Scroll the page to the bottom most of a screen at a time, as a
    /// visitor does, pausing `pause` after each step. With `frames_to`, the
    /// first step is photographed as its reveal plays.
    async fn scroll_through(
        &mut self,
        session: &str,
        pause: Duration,
        frames_to: Option<&Path>,
        frames: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let scroller = self.evaluate(session, MOTION_SCROLLER).await?;
        let max = scroller["max"].as_u64().unwrap_or(0);
        let step = scroller["step"].as_u64().unwrap_or(600).max(200);
        let mut y = 0;
        let mut steps = 0;
        while y < max && steps < 60 {
            y = (y + step).min(max);
            steps += 1;
            self.evaluate(
                session,
                &format!("window.__enxMotion && window.__enxMotion.scrollTo({y})"),
            )
            .await?;
            match (steps, frames_to) {
                (1, Some(dir)) => {
                    let moved = tokio::time::Instant::now();
                    for at in [100u64, 700] {
                        tokio::time::sleep_until(moved + Duration::from_millis(at)).await;
                        if let Ok(path) = self
                            .frame(session, dir, &format!("motion-scroll-{at}ms.png"))
                            .await
                        {
                            frames.push(path);
                        }
                    }
                }
                _ => tokio::time::sleep(pause).await,
            }
        }
        Ok(())
    }

    /// Measure the page on screen, and save its screenshot as `shot`.
    async fn measure(
        &mut self,
        session: &str,
        width: u32,
        out_dir: &Path,
        shot: &str,
    ) -> Result<WidthReport> {
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
            low_contrast_graphics: strings("low_contrast_graphics"),
            theme: None,
            console_errors: self.console_errors(session),
            screenshot: None,
        };
        let height = (report.page_height as u32).clamp(1, MAX_SHOT_HEIGHT);
        let capture = self
            .call(
                "Page.captureScreenshot",
                json!({"format": "png", "captureBeyondViewport": true,
                       "clip": {"x": 0, "y": 0, "width": width, "height": height, "scale": 1}}),
                Some(session),
            )
            .await?;
        if let Some(data) = capture["data"].as_str() {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
            let path = out_dir.join(shot);
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
            "\n{}px{}, page {}px tall: {}\n",
            r.width,
            r.theme
                .as_deref()
                .map(|theme| format!(" in the {theme} theme"))
                .unwrap_or_default(),
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
        list(
            "drawings and icons below 3:1 (draw them with currentColor and a colour token per \
             theme, ui-themes)",
            &r.low_contrast_graphics,
        );
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

/// A length of time as the report writes it: `700ms`, `2.5s`.
fn seconds(ms: u64) -> String {
    if ms >= 1000 {
        format!("{}s", (ms as f64 / 100.0).round() / 10.0)
    } else {
        format!("{ms}ms")
    }
}

fn strings_of(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A requestAnimationFrame loop, from the frames counted over a second at
/// the top of the page and over a second at the bottom: whether it stops
/// when scrolled away.
fn raf_loop(top: u64, bottom: u64, screens: f64, who: &str) -> Option<(String, bool)> {
    let from = if who.is_empty() {
        String::new()
    } else {
        format!(" ({who})")
    };
    if top >= 20 {
        if screens < 1.5 {
            return Some((
                format!("requestAnimationFrame about {top} times a second{from}"),
                false,
            ));
        }
        if bottom >= 20 {
            return Some((
                format!(
                    "requestAnimationFrame about {top} times a second at the top of the page and \
                     still {bottom} with it scrolled to the bottom: it never stops{from}"
                ),
                true,
            ));
        }
        return Some((
            format!(
                "requestAnimationFrame about {top} times a second at the top of the page, \
                 stopping once it is scrolled away{from}"
            ),
            false,
        ));
    }
    (bottom >= 20).then(|| {
        (
            format!("requestAnimationFrame about {bottom} times a second at the bottom of the page{from}"),
            false,
        )
    })
}

/// Fill `report` from the look at the page as it is: `rest` is a second at
/// the top after loading, `found` everything after scrolling through.
fn read_motion(report: &mut MotionReport, rest: &Value, found: &Value) {
    if found.is_null() {
        report.note = Some("the page's scripts stopped answering while it was scrolled".into());
        return;
    }
    report.on_load = strings_of(&found["load"]);
    report.load_count = found["load_count"].as_u64().unwrap_or(0) as usize;
    report.load_ends_ms = found["load_ends"].as_u64().unwrap_or(0);
    report.on_scroll = found["scroll_count"].as_u64().unwrap_or(0) as usize;
    report.scroll_blocks = found["blocks"].as_u64().unwrap_or(0) as usize;
    report.longest_sequence = found["longest"].as_str().map(str::to_owned);
    report.endless = strings_of(&found["endless"]);
    let who = rest["who"]
        .as_str()
        .filter(|w| !w.is_empty())
        .or(found["who"].as_str())
        .unwrap_or("");
    if let Some((line, never_stops)) = raf_loop(
        rest["raf"].as_u64().unwrap_or(0),
        found["raf"].as_u64().unwrap_or(0),
        found["screens"].as_f64().unwrap_or(1.0),
        who,
    ) {
        report.loops.push(line);
        report.never_stops += usize::from(never_stops);
    }
    report.loops.extend(strings_of(&found["timers"]));
    report.layout_animated = strings_of(&found["layout"]);
    report.transition_all = strings_of(&found["all"]);
    report.transition_all_count = found["all_count"].as_u64().unwrap_or(0) as usize;
    let mut writers: Vec<String> = strings_of(&rest["writers"])
        .into_iter()
        .map(|w| format!("{w} at the top of the page"))
        .collect();
    writers.extend(
        strings_of(&found["writers"])
            .into_iter()
            .map(|w| format!("{w} at the bottom of the page")),
    );
    report.js_animated = writers;
    report.long_frames = found["long"].as_str().map(str::to_owned);
    report.layout_shift = found["shift"].as_str().map(str::to_owned);
    report.hidden = strings_of(&found["hidden"]);
    if let Some(listeners) = found["listeners"].as_array() {
        for listener in listeners {
            if let Some(text) = listener["text"].as_str() {
                report.listeners.push(text.to_owned());
            }
            report.blocking_listeners += usize::from(listener["blocking"] == true);
        }
    }
    report.observers = found["observers"].as_str().map(str::to_owned);
}

/// Fill in what the page did with reduced motion.
fn read_reduced(report: &mut MotionReport, rest: &Value, found: &Value) {
    if found.is_null() {
        return;
    }
    report.reduced_moving = strings_of(&found["moving"]);
    report.reduced_loops = strings_of(&found["endless"]);
    if let Some((line, _)) = raf_loop(
        rest["raf"].as_u64().unwrap_or(0),
        0,
        1.0,
        rest["who"].as_str().unwrap_or(""),
    ) {
        report.reduced_loops.push(line);
    }
    report.reduced_hidden = strings_of(&found["hidden"]);
    report.reduced_smooth_scroll = found["smooth"] == true;
}

/// How the page moved, as the agent reads it.
pub fn motion_report(m: &MotionReport) -> String {
    let mut out = format!(
        "\nMotion at {}px: {}\n",
        m.width,
        match m.problems() {
            0 => "nothing wrong found".to_owned(),
            1 => "1 problem".to_owned(),
            n => format!("{n} problems"),
        }
    );
    if let Some(note) = &m.note {
        out.push_str(&format!("  {note}\n"));
        return out;
    }
    let list = |out: &mut String, label: &str, items: &[String]| {
        if !items.is_empty() {
            out.push_str(&format!("  {label}:\n"));
            for item in items {
                out.push_str(&format!("    - {item}\n"));
            }
        }
    };
    if m.on_load.is_empty() {
        out.push_str("  nothing animates on load\n");
    } else {
        out.push_str(&format!(
            "  on load, {} animation{}, the last ending at {} (times from the start of loading):\n",
            m.load_count,
            if m.load_count == 1 { "" } else { "s" },
            seconds(m.load_ends_ms)
        ));
        for line in &m.on_load {
            out.push_str(&format!("    {line}\n"));
        }
    }
    if m.on_scroll == 0 {
        out.push_str("  nothing animates as the page is scrolled through\n");
    } else {
        out.push_str(&format!(
            "  scrolling through the page started {} animation{} in {} block{}{}\n",
            m.on_scroll,
            if m.on_scroll == 1 { "" } else { "s" },
            m.scroll_blocks,
            if m.scroll_blocks == 1 { "" } else { "s" },
            m.longest_sequence
                .as_deref()
                .map(|run| format!("; the longest, {run}"))
                .unwrap_or_default()
        ));
    }
    list(
        &mut out,
        "repeating forever (each needs a job, such as a caret or a spinner while waiting)",
        &m.endless,
    );
    list(
        &mut out,
        "script loops (a loop pauses when its part of the page is off screen: motion-performance)",
        &m.loops,
    );
    list(
        &mut out,
        "animating layout or heavy paint (move with transform and opacity instead: motion-performance)",
        &m.layout_animated,
    );
    if m.transition_all_count > 0 {
        out.push_str(&format!(
            "  `transition: all` (or a transition with no property) in {} rule{}, such as {}: name the properties it animates\n",
            m.transition_all_count,
            if m.transition_all_count == 1 { "" } else { "s" },
            m.transition_all.join(", ")
        ));
    }
    list(&mut out, "animated from a script", &m.js_animated);
    if let Some(frames) = &m.long_frames {
        out.push_str(&format!(
            "  long frames: {frames} (measured in headless Chrome; a hint, not a benchmark)\n"
        ));
    }
    if let Some(shift) = &m.layout_shift {
        out.push_str(&format!(
            "  layout shift while loading and revealing: {shift}\n"
        ));
    }
    list(
        &mut out,
        "still hidden after scrolling through the whole page, content that never appears (motion-reveal)",
        &m.hidden,
    );
    list(
        &mut out,
        "scroll, wheel and pointer listeners",
        &m.listeners,
    );
    if let Some(observers) = &m.observers {
        out.push_str(&format!("  {observers}\n"));
    }
    let reduced = !m.reduced_moving.is_empty()
        || !m.reduced_loops.is_empty()
        || !m.reduced_hidden.is_empty()
        || m.reduced_smooth_scroll;
    if reduced {
        out.push_str("  with reduced motion (motion-comfort):\n");
        list(&mut out, "  still moves", &m.reduced_moving);
        list(&mut out, "  still loops", &m.reduced_loops);
        list(&mut out, "  hidden", &m.reduced_hidden);
        if m.reduced_smooth_scroll {
            out.push_str("    smooth scrolling is still on: keep it for no-preference only\n");
        }
    } else {
        out.push_str("  with reduced motion: nothing moves, loops or stays hidden\n");
    }
    if let Some(first) = m.frames.first() {
        out.push_str(&format!(
            "  frames of the first seconds and of the first reveal, for the user: {} ({} images)\n",
            first
                .parent()
                .map(|dir| dir.display().to_string())
                .unwrap_or_default(),
            m.frames.len()
        ));
    }
    out.push_str("  `motion-audit` says what each finding means and how to fix it.\n");
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

/// Run in the page: the lightness of its background, 0 to 100, from the
/// body or the root, else from what sits in the window's bottom corner.
const PAGE_LIGHTNESS: &str = r#"(() => {
  const parse = c => {
    const m = c && c.match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const p = m[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 };
  };
  const solid = el => { for (let e = el; e; e = e.parentElement) { const c = parse(getComputedStyle(e).backgroundColor); if (c && c.a > 0.95) return c; } return null; };
  const c = solid(document.body) || solid(document.elementFromPoint(innerWidth - 4, innerHeight - 4));
  if (!c) return null;
  return (Math.max(c.r, c.g, c.b) + Math.min(c.r, c.g, c.b)) / 2 / 255 * 100;
})()"#;

/// Run in the page: switch it to `__THEME__` the usual ways a toggle does,
/// a class or an attribute on the root.
const FLIP_THEME: &str = r#"(async () => {
  const want = '__THEME__';
  for (const el of [document.documentElement, document.body]) {
    el.classList.toggle('dark', want === 'dark');
    el.classList.toggle('light', want === 'light');
  }
  for (const attr of ['data-theme', 'data-mode', 'data-color-scheme', 'data-bs-theme']) {
    document.documentElement.setAttribute(attr, want);
  }
  document.documentElement.style.colorScheme = want;
  await new Promise(done => setTimeout(done, 400));
  return true;
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
  // Drawings and icons: the colour most of each visible SVG is drawn in,
  // against what is behind it, at 3:1. A drawing left in the SVG default,
  // black, disappears on a dark page. A large decoration spread behind the
  // content is exempt.
  out.low_contrast_graphics = [];
  const hexOf = c => '#' + [c.r, c.g, c.b].map(v => Math.round(v).toString(16).padStart(2, '0')).join('');
  // The colour most of a drawing's strokes and fills are made of.
  const mainPaint = svg => {
    const counts = new Map();
    for (const shape of [...svg.querySelectorAll('path, line, polyline, polygon, rect, circle, ellipse, text, use')].slice(0, 300)) {
      const cs = getComputedStyle(shape);
      if (cs.display === 'none' || cs.visibility === 'hidden') continue;
      const paints = [];
      if (cs.stroke && cs.stroke !== 'none' && parseFloat(cs.strokeWidth) > 0 && parseFloat(cs.strokeOpacity) > 0.2) paints.push(cs.stroke);
      if (cs.fill && cs.fill !== 'none' && parseFloat(cs.fillOpacity) > 0.2) paints.push(cs.fill);
      for (const paint of paints) {
        const c = parse(paint);
        if (!c || c.a < 0.2) continue;
        const key = hexOf(c);
        counts.set(key, { c, n: ((counts.get(key) || {}).n || 0) + 1 });
      }
    }
    return counts.size ? [...counts.values()].sort((x, y) => y.n - x.n)[0].c : null;
  };
  // A drawing's contrast against what is behind `el`, or null when it holds.
  const weak = (el, main) => {
    const bg = background(el);
    if (!bg || !main) return null;
    const drawn = { r: main.r * main.a + bg.r * (1 - main.a), g: main.g * main.a + bg.g * (1 - main.a), b: main.b * main.a + bg.b * (1 - main.a) };
    const l1 = lum(drawn), l2 = lum(bg);
    const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    return ratio < 3 ? ' drawn in ' + hexOf(drawn) + ' on ' + hexOf(bg) + ', ' + ratio.toFixed(2) + ':1, needs 3:1' : null;
  };
  const worthChecking = (el, min) => {
    if (!visible(el)) return false;
    const box = el.getBoundingClientRect();
    // Long and thin still counts: a dial or a rule 40 by 14.
    if (Math.max(box.width, box.height) < min || Math.min(box.width, box.height) < 8) return false;
    const st = getComputedStyle(el);
    // A large decoration spread behind the content is exempt.
    if (/absolute|fixed/.test(st.position) && box.width > W * 0.5) return false;
    return parseFloat(st.opacity) >= 0.3;
  };
  const size = el => { const b = el.getBoundingClientRect(); return Math.round(b.width) + 'x' + Math.round(b.height); };
  // Inline drawings and icons, in the page's own colours. A drawing left in
  // the SVG default, black, disappears on a dark page.
  for (const svg of document.querySelectorAll('svg')) {
    if (out.low_contrast_graphics.length >= 6) break;
    if (svg.parentElement && svg.parentElement.closest('svg')) continue;
    if (!worthChecking(svg, 12)) continue;
    const found = weak(svg, mainPaint(svg));
    if (found) out.low_contrast_graphics.push(describe(svg) + ' ' + size(svg) + found);
  }
  // Drawings loaded with <img>. An image never takes the page's colours:
  // `currentColor` in it is black, whatever the theme. Each is drawn once
  // off screen with black as its colour, as the image renders it.
  const offscreen = document.createElement('div');
  offscreen.style.cssText = 'position:absolute;left:-10000px;top:0;width:800px;color:#000;visibility:visible';
  document.body.appendChild(offscreen);
  for (const img of document.querySelectorAll('img')) {
    if (out.low_contrast_graphics.length >= 6) break;
    const src = img.currentSrc || img.src || '';
    if (!/\.svg([?#]|$)/i.test(src) && !src.startsWith('data:image/svg+xml')) continue;
    if (!worthChecking(img, 24)) continue;
    let text = '';
    try { text = await (await fetch(src)).text(); } catch (e) { continue; }
    offscreen.innerHTML = text;
    const svg = offscreen.querySelector('svg');
    if (!svg) continue;
    const found = weak(img, mainPaint(svg));
    if (found) {
      const name = src.startsWith('data:') ? 'an inline data SVG' : src.split('/').pop().split(/[?#]/)[0];
      out.low_contrast_graphics.push('img ' + name + ' ' + size(img) + found
        + ' (an SVG in an <img> keeps its own colours: inline it, or give each theme its file)');
    }
  }
  offscreen.remove();
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

/// Installed before the page's own scripts when its motion is watched:
/// records each CSS animation, transition and `element.animate()` as it
/// starts (what it animates, how far, how long), counts requestAnimationFrame
/// callbacks and inline style rewrites, notes fast timers, scroll and pointer
/// listeners and IntersectionObservers, and keeps long frames and layout
/// shifts. Read back by `MOTION_AT_REST` and `MOTION_REPORT`.
const MOTION_HOOKS: &str = r##"(() => {
  if (window.__enxMotion) return;
  const M = window.__enxMotion = {
    started: [], raf: 0, rafWho: '', timers: [], longFrames: [], shifts: [],
    listeners: [], observers: 0, observed: 0, writes: new Map(), phase: 'load',
  };
  const add = EventTarget.prototype.addEventListener;
  const describe = el => {
    if (el === window) return 'window';
    if (el === document) return 'document';
    if (!el || el.nodeType !== 1) return '?';
    const tag = el.tagName.toLowerCase();
    if (el.id) return tag + '#' + el.id;
    const classes = (el.getAttribute('class') || '').trim().split(/\s+/).filter(Boolean).slice(0, 2);
    if (classes.length) return tag + '.' + classes.join('.');
    const parent = el.parentElement;
    return parent && parent !== document.body && parent !== document.documentElement ? describe(parent) + ' ' + tag : tag;
  };
  M.describe = describe;
  // The page's own file and line that called into a hook.
  const caller = () => {
    for (const line of (new Error().stack || '').split('\n')) {
      const m = line.match(/((?:https?|file):\/\/[^\s)]+?):(\d+):\d+/);
      if (m) return m[1].split('/').pop().split('?')[0] + ':' + m[2];
    }
    return '';
  };
  const raf = window.requestAnimationFrame;
  let calls = 0;
  window.requestAnimationFrame = function (callback) {
    if (++calls % 30 === 1) { const at = caller(); if (at) M.rafWho = at; }
    return raf.call(window, time => { M.raf++; return callback(time); });
  };
  const every = window.setInterval;
  window.setInterval = function (fn, ms, ...rest) {
    if (typeof ms === 'number' && ms < 200) {
      const at = caller();
      if (!M.timers.some(t => t.at === at && t.ms === ms)) M.timers.push({ ms: Math.max(0, Math.round(ms)), at });
    }
    return every.call(window, fn, ms, ...rest);
  };
  EventTarget.prototype.addEventListener = function (type, fn, options) {
    if (/^(scroll|wheel|touchmove|mousemove|pointermove)$/.test(type)) {
      const target = describe(this);
      const passive = typeof options === 'object' && options !== null ? options.passive : undefined;
      const key = type + ' ' + target;
      if (!M.listeners.some(l => l.key === key)) M.listeners.push({ key, type, target, passive });
    }
    return add.call(this, type, fn, options);
  };
  if (window.IntersectionObserver) {
    const Native = window.IntersectionObserver;
    // What an observer reports coming into view is what a reveal starts
    // from: scroll animations are grouped by the nearest one.
    M.revealed = new WeakSet();
    window.IntersectionObserver = class extends Native {
      constructor(callback, options) {
        super((entries, observer) => {
          for (const e of entries) if (e.isIntersecting) M.revealed.add(e.target);
          return callback(entries, observer);
        }, options);
        M.observers++;
      }
      observe(target) { M.observed++; return super.observe(target); }
    };
  }
  try {
    new PerformanceObserver(list => {
      for (const e of list.getEntries()) {
        if (e.duration < 100) continue;
        const script = [...(e.scripts || [])].sort((a, b) => b.duration - a.duration)[0];
        const source = script ? String(script.sourceURL || script.invoker || '').split('/').pop().split('?')[0] : '';
        M.longFrames.push({ duration: Math.round(e.duration), phase: M.phase, source });
      }
    }).observe({ type: 'long-animation-frame', buffered: true });
  } catch (e) {}
  try {
    new PerformanceObserver(list => {
      for (const e of list.getEntries()) {
        if (e.hadRecentInput) continue;
        M.shifts.push({ value: e.value, sources: [...(e.sources || [])].map(s => s.node && s.node.nodeType === 1 ? describe(s.node) : '').filter(Boolean) });
      }
    }).observe({ type: 'layout-shift', buffered: true });
  } catch (e) {}
  new MutationObserver(records => {
    for (const r of records) M.writes.set(r.target, (M.writes.get(r.target) || 0) + 1);
  }).observe(document, { subtree: true, attributes: true, attributeFilter: ['style'] });
  const kebab = p => p.replace(/[A-Z]/g, c => '-' + c.toLowerCase());
  // How far a set of keyframes moves, scales and turns its element.
  const motionOf = frames => {
    let px = 0, pct = 0, scale = 1, rotate = false;
    const nums = s => [...String(s).matchAll(/(-?[\d.]+)(px|%)?/g)].map(m => ({ n: Math.abs(parseFloat(m[1])), unit: m[2] || '' }));
    const far = list => { for (const x of list) { if (x.unit === 'px') px = Math.max(px, x.n); else if (x.unit === '%') pct = Math.max(pct, x.n); } };
    for (const f of frames) {
      const t = f.transform && f.transform !== 'none' ? String(f.transform) : '';
      for (const m of t.matchAll(/translate(?:3d|X|Y|Z)?\(([^)]*)\)/g)) far(nums(m[1]));
      for (const m of t.matchAll(/matrix\(([^)]*)\)/g)) {
        const n = m[1].split(',').map(parseFloat);
        px = Math.max(px, Math.abs(n[4] || 0), Math.abs(n[5] || 0));
        if (Math.abs(n[1] || 0) > 0.001 || Math.abs(n[2] || 0) > 0.001) rotate = true;
        else scale = Math.min(scale, n[0], n[3]);
      }
      for (const m of t.matchAll(/scale(?:X|Y|3d)?\(([^)]*)\)/g)) for (const x of nums(m[1])) scale = Math.min(scale, x.n);
      if (/rotate/.test(t) && !/rotate[XYZ]?\(0(deg|turn|rad)?\)/.test(t)) rotate = true;
      if (f.translate && f.translate !== 'none') far(nums(f.translate));
      if (f.scale && f.scale !== 'none') for (const x of nums(f.scale)) scale = Math.min(scale, x.n);
      if (f.rotate && f.rotate !== 'none' && !/^0(deg)?$/.test(String(f.rotate).trim())) rotate = true;
    }
    return { px: Math.round(px), pct: Math.round(pct), scale: Math.round(scale * 100) / 100, rotate };
  };
  const note = (anim, target, pseudo, kind) => {
    if (M.started.length > 3000 || !anim || !anim.effect) return;
    const effect = anim.effect;
    const timing = effect.getTiming ? effect.getTiming() : {};
    let frames = [];
    try { frames = effect.getKeyframes(); } catch (e) {}
    const props = new Set();
    for (const f of frames) for (const k of Object.keys(f)) if (!/^(offset|computedOffset|easing|composite)$/.test(k)) props.add(kebab(k));
    M.started.push({
      at: Math.round(performance.now()) + (kind === 'js' ? Math.round(Number(timing.delay) || 0) : 0),
      target, el: describe(target) + (pseudo || ''),
      name: anim.animationName || anim.transitionProperty || anim.id || '',
      props: [...props],
      duration: Math.round(Number(timing.duration) || 0),
      iterations: timing.iterations,
      move: motionOf(frames), phase: M.phase,
    });
  };
  const find = (target, pseudo, pick) => {
    let list = [];
    try { list = target.getAnimations({ subtree: !!pseudo }); } catch (e) { return null; }
    return list.find(a => pick(a) && (!pseudo || (a.effect && a.effect.pseudoElement === pseudo))) || null;
  };
  add.call(document, 'animationstart', e => {
    note(find(e.target, e.pseudoElement, a => a.animationName === e.animationName), e.target, e.pseudoElement, 'css');
  }, true);
  add.call(document, 'transitionstart', e => {
    note(find(e.target, e.pseudoElement, a => a.transitionProperty === e.propertyName), e.target, e.pseudoElement, 'transition');
  }, true);
  const animate = Element.prototype.animate;
  Element.prototype.animate = function (...args) {
    const a = animate.apply(this, args);
    try { note(a, this, '', 'js'); } catch (e) {}
    return a;
  };
})()"##;

/// A second at rest: requestAnimationFrame callbacks and inline style
/// rewrites counted over it. Null when the hooks never ran.
const MOTION_AT_REST: &str = r##"(async () => {
  const M = window.__enxMotion;
  if (!M) return null;
  M.writes = new Map();
  const before = M.raf;
  await new Promise(done => setTimeout(done, 1000));
  const writers = [...M.writes].filter(([, n]) => n >= 20).sort((a, b) => b[1] - a[1]).slice(0, 4)
    .map(([el, n]) => M.describe(el) + ' rewrites its style ' + n + ' times a second');
  return { raf: M.raf - before, who: M.rafWho, writers };
})()"##;

/// Find what scrolls the page (the window, or the largest scrolling box
/// when the page scrolls inside one), turn smooth scrolling off for the
/// visit, and say how far and in what steps to go.
const MOTION_SCROLLER: &str = r##"(() => {
  const M = window.__enxMotion;
  if (!M) return null;
  const root = document.scrollingElement || document.documentElement;
  let scroller = root;
  if (root.scrollHeight <= innerHeight + 20) {
    let area = 0;
    for (const el of document.querySelectorAll('body *')) {
      if (el.scrollHeight <= el.clientHeight + 20) continue;
      const overflow = getComputedStyle(el).overflowY;
      if (overflow !== 'auto' && overflow !== 'scroll' && overflow !== 'overlay') continue;
      const r = el.getBoundingClientRect();
      if (r.width * r.height > area) { area = r.width * r.height; scroller = el; }
    }
  }
  M.scroller = scroller;
  M.smooth = [scroller, root, document.body].some(el => el && getComputedStyle(el).scrollBehavior === 'smooth');
  for (const el of new Set([scroller, root])) el.style.scrollBehavior = 'auto';
  M.scrollTo = y => scroller.scrollTo(0, y);
  M.phase = 'scroll';
  const height = scroller === root ? innerHeight : scroller.clientHeight;
  M.screens = Math.round(scroller.scrollHeight / height * 10) / 10;
  return { max: Math.max(0, scroller.scrollHeight - height), step: Math.max(200, Math.round(height * 0.75)) };
})()"##;

/// After scrolling through: a second at the bottom, then everything the
/// hooks saw, summarised and worded for the report.
const MOTION_REPORT: &str = r##"(async () => {
  const M = window.__enxMotion;
  if (!M) return null;
  M.writes = new Map();
  const before = M.raf;
  await new Promise(done => setTimeout(done, 1000));
  const out = { raf: M.raf - before, who: M.rafWho, screens: M.screens || 1, smooth: !!M.smooth };
  out.writers = [...M.writes].filter(([, n]) => n >= 20).sort((a, b) => b[1] - a[1]).slice(0, 4)
    .map(([el, n]) => M.describe(el) + ' rewrites its style ' + n + ' times a second');
  const seconds = ms => (ms >= 1000 ? (Math.round(ms / 100) / 10) + 's' : Math.round(ms) + 'ms');
  const what = s => {
    const parts = [];
    if (s.props.includes('opacity')) parts.push('opacity');
    if (s.move.px) parts.push('moves ' + s.move.px + 'px');
    if (s.move.pct) parts.push('moves ' + s.move.pct + '%');
    if (s.move.scale !== 1) parts.push('scales from ' + s.move.scale);
    if (s.move.rotate) parts.push('rotates');
    parts.push(...s.props.filter(p => !/^(transform|translate|scale|rotate|opacity)$/.test(p)).slice(0, 3));
    return parts.join(', ') || s.name;
  };
  const moves = s => s.move.px >= 2 || s.move.pct >= 2 || Math.abs(s.move.scale - 1) >= 0.02 || s.move.rotate;
  // On load, in the order they start; the same move on the same element
  // counted once.
  const load = M.started.filter(s => s.phase === 'load');
  const lines = new Map();
  for (const s of load) {
    const key = s.el + '|' + what(s) + '|' + s.duration;
    const line = lines.get(key);
    if (line) line.count++;
    else lines.set(key, { at: s.at, count: 1, text: 'for ' + seconds(s.duration) + (s.iterations === Infinity ? ', repeating' : '') + ': ' + s.el + ', ' + what(s) });
  }
  out.load = [...lines.values()].sort((a, b) => a.at - b.at).slice(0, 14)
    .map(l => 'at ' + seconds(l.at) + ' ' + l.text + (l.count > 1 ? ' (x' + l.count + ')' : ''));
  out.load_count = load.length;
  out.load_ends = Math.round(Math.max(0, ...load.filter(s => s.iterations !== Infinity).map(s => s.at + s.duration * (s.iterations || 1))));
  // On scroll: animations grouped by the block whose reveal started them
  // (the nearest element an IntersectionObserver reported), or by bursts
  // close in time when no observer is involved. The longest group is named.
  const scrolled = M.started.filter(s => s.phase === 'scroll').sort((a, b) => a.at - b.at);
  out.scroll_count = scrolled.length;
  const groups = new Map();
  let burst = null, last = -Infinity;
  for (const s of scrolled) {
    let block = null;
    if (M.revealed) for (let a = s.target; a && a.nodeType === 1; a = a.parentElement) if (M.revealed.has(a)) { block = a; break; }
    if (!block) {
      if (!burst || s.at - last > 400) burst = { burst: true };
      last = s.at;
      block = burst;
    }
    if (!groups.has(block)) groups.set(block, []);
    groups.get(block).push(s);
  }
  out.blocks = groups.size;
  out.longest = null;
  let longest = 0;
  for (const [block, list] of groups) {
    if (list.length < 2) continue;
    const span = Math.max(...list.map(s => s.at + s.duration)) - list[0].at;
    if (span <= longest) continue;
    longest = span;
    let name = block;
    if (block.burst) {
      const els = list.map(s => s.target).filter(el => el && el.isConnected);
      name = els[0];
      while (name && !els.every(el => name.contains(el))) name = name.parentElement;
    }
    out.longest = (name && name.nodeType === 1 ? M.describe(name) : 'the page') + ': ' + list.length + ' animations over ' + seconds(span);
  }
  out.endless = [];
  for (const a of document.getAnimations()) {
    const effect = a.effect;
    if (!effect || a.playState !== 'running') continue;
    const timing = effect.getTiming();
    if (timing.iterations !== Infinity) continue;
    const text = (effect.target ? M.describe(effect.target) : '?') + (effect.pseudoElement || '') + ': '
      + (a.animationName || a.id || 'an animation') + ' every ' + seconds(Number(timing.duration) || 0);
    if (!out.endless.includes(text)) out.endless.push(text);
    if (out.endless.length >= 6) break;
  }
  const LAYOUT = /^(width|height|min-width|min-height|max-width|max-height|top|left|right|bottom|inset|margin|margin-\w+|padding|padding-\w+|font-size|line-height|letter-spacing|border-width|border-\w+-width|gap|row-gap|column-gap|grid-template-rows|grid-template-columns|flex-basis)$/;
  const HEAVY = /^(box-shadow|filter|backdrop-filter)$/;
  out.layout = [];
  for (const s of M.started) {
    const costly = s.props.filter(p => LAYOUT.test(p) || HEAVY.test(p));
    if (!costly.length || !s.target || !s.target.isConnected) continue;
    const cs = getComputedStyle(s.target);
    const r = s.target.getBoundingClientRect();
    const small = r.width * r.height < 20000;
    // A shadow or a filter on something small is cheap to repaint, and so
    // is resizing a small element taken out of the flow.
    if (small && costly.every(p => HEAVY.test(p))) continue;
    if (small && /absolute|fixed/.test(cs.position)) continue;
    const text = s.el + ': ' + costly.join(', ');
    if (!out.layout.includes(text)) out.layout.push(text);
    if (out.layout.length >= 6) break;
  }
  const all = new Set();
  const walk = rules => {
    for (const rule of rules) {
      if (rule.cssRules && !rule.selectorText) { walk(rule.cssRules); continue; }
      const style = rule.style;
      if (!style || !rule.selectorText) continue;
      if (!(style.transitionProperty || '').split(',').some(p => p.trim() === 'all')) continue;
      const durations = (style.transitionDuration || '').split(',');
      if (durations.every(d => !(parseFloat(d) > 0))) continue;
      all.add(rule.selectorText);
    }
  };
  for (const sheet of document.styleSheets) { try { walk(sheet.cssRules); } catch (e) {} }
  out.all = [...all].slice(0, 5);
  out.all_count = all.size;
  const long = M.longFrames.filter(f => f.phase === 'scroll');
  out.long = null;
  if (long.length >= 2) {
    const worst = long.reduce((a, b) => (b.duration > a.duration ? b : a));
    out.long = long.length + ' frames of 100ms or more while scrolling, the longest ' + worst.duration + 'ms' + (worst.source ? ' (' + worst.source + ')' : '');
  }
  const shift = M.shifts.reduce((sum, s) => sum + s.value, 0);
  out.shift = null;
  if (shift > 0.05) {
    const blame = new Map();
    for (const s of M.shifts) for (const source of s.sources) blame.set(source, (blame.get(source) || 0) + s.value);
    const top = [...blame].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([el]) => el);
    out.shift = (Math.round(shift * 100) / 100) + (top.length ? ', mostly ' + top.join(', ') : '');
  }
  // Text still invisible now that the whole page has been scrolled past:
  // content that never appears. What is hidden on purpose until asked for
  // (menus, tooltips, closed dialogs, text for screen readers, rows that
  // scroll sideways) is left out.
  out.hidden = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const seen = new Set();
  let looked = 0;
  while (walker.nextNode() && looked < 4000 && out.hidden.length < 6) {
    const text = walker.currentNode.textContent.trim();
    const el = walker.currentNode.parentElement;
    if (text.length < 2 || !el || seen.has(el)) continue;
    seen.add(el);
    looked++;
    if (el.closest('[aria-hidden="true"], [inert], [hidden], dialog:not([open]), [popover], [role=tooltip], [role=menu], [role=listbox], details:not([open]), noscript, template, script, style')) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1 || r.right <= 0 || r.left >= innerWidth) continue;
    let opacity = 1, skip = false;
    for (let a = el; a && a.nodeType === 1; a = a.parentElement) {
      const cs = getComputedStyle(a);
      if (cs.display === 'none' || cs.clipPath === 'inset(50%)' || (cs.position === 'absolute' && cs.clip && cs.clip !== 'auto')) { skip = true; break; }
      const o = parseFloat(cs.opacity);
      if (o < 0.05 && cs.pointerEvents === 'none') { skip = true; break; }
      opacity *= o;
    }
    if (skip) continue;
    let aside = false;
    for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
      if (getComputedStyle(a).overflowX === 'visible') continue;
      const box = a.getBoundingClientRect();
      if (r.left >= box.right || r.right <= box.left) { aside = true; break; }
    }
    if (aside) continue;
    const hiddenBy = getComputedStyle(el).visibility === 'hidden' ? 'visibility: hidden'
      : opacity < 0.05 ? 'opacity ' + (Math.round(opacity * 100) / 100) : '';
    if (hiddenBy) out.hidden.push(M.describe(el) + ' "' + text.slice(0, 40) + '" (' + hiddenBy + ')');
  }
  // A framework's event system (React's root) listens for all of these at
  // once on one element; that is plumbing, not a scroll effect.
  const perTarget = new Map();
  for (const l of M.listeners) perTarget.set(l.target, (perTarget.get(l.target) || 0) + 1);
  out.listeners = M.listeners.filter(l => perTarget.get(l.target) < 4).slice(0, 8).map(l => {
    const blocking = (l.type === 'wheel' || l.type === 'touchmove') && l.passive === false;
    return { text: l.type + ' on ' + l.target + (blocking ? ', not passive: it can hold up or take over scrolling' : ''), blocking };
  });
  out.observers = M.observers
    ? M.observers + ' IntersectionObserver' + (M.observers === 1 ? '' : 's') + ' watching ' + M.observed + ' element' + (M.observed === 1 ? '' : 's')
    : null;
  out.timers = M.timers.slice(0, 4).map(t => 'setInterval every ' + t.ms + 'ms' + (t.at ? ' (' + t.at + ')' : ''));
  out.moving = [...new Set(M.started.filter(moves).map(s => s.el + ', ' + what(s) + ' for ' + seconds(s.duration)))].slice(0, 8);
  return out;
})()"##;
