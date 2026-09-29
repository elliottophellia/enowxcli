//! A sample of fuller motion in the terminal interface, played on the real
//! interface before any of it is built in.
//!
//! The screen is the real interface, drawn by `TestApp` and fed a scripted
//! turn through the events a running turn sends: a message, thinking, a file
//! read, three delegations running side by side (one fails and is retried), a
//! test run and a streamed report. The motion is laid over the frame the
//! interface drew, so the interface itself is unchanged.
//!
//! Run it in a terminal at least 100 columns wide:
//!
//!   cargo run -q -p enowx-tui --example motion_sample
//!
//! Keys: r replay · m motion full / reduced / off · s smooth streaming on or
//! off · / the command list (Esc closes it) · 1-4 sidebar tabs · q quit.
//!
//! The bottom row counts the frames drawn and the bytes written to the
//! terminal in the last second. Nothing is drawn while nothing moves.

use std::cell::Cell;
use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::io::{self, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as Input, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
    EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, execute};
use enowx_core::Event;
use enowx_tui::testing::TestApp;
use enowx_tui::theme::Theme;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::Terminal;

/// A frame while something moves. Steps of one cell look smooth at 30 a
/// second, and a terminal takes far more than that.
const FRAME: Duration = Duration::from_millis(33);
/// A frame for slow loops: a breathing marker.
const SLOW_FRAME: Duration = Duration::from_millis(66);
/// A frame for the shimmer on the status label.
const SHIMMER_FRAME: Duration = Duration::from_millis(40);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPIN_MS: u128 = 90;
/// The status markers the interface draws on tool and delegation rows.
const MARKS: [&str; 4] = ["›", "✓", "✗", "◆"];
/// The write position while a reply streams.
const CARET: char = '▍';
/// How long a newly shown character takes to reach its colour.
const TRAIL_MS: f32 = 180.0;
/// Characters the markdown renderer hides, so they have no cell to fade.
const HIDDEN: [char; 3] = ['*', '`', '#'];
const WINDOW: u32 = 128_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Everything in the sample.
    Full,
    /// State changes land at once; only the spinner on a running call moves.
    Reduced,
    /// The interface as it is today.
    Off,
}

impl Mode {
    fn next(self) -> Self {
        match self {
            Mode::Full => Mode::Reduced,
            Mode::Reduced => Mode::Off,
            Mode::Off => Mode::Full,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Mode::Full => "full",
            Mode::Reduced => "reduced",
            Mode::Off => "off",
        }
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// How far an effect that started `at` and lasts `ms` has come, or None once
/// it is over.
fn progress(at: Instant, now: Instant, ms: f32) -> Option<f32> {
    let p = now.saturating_duration_since(at).as_secs_f32() * 1000.0 / ms;
    (p < 1.0).then_some(p)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return if t < 0.5 { a } else { b };
    };
    let t = t.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::Rgb(channel(ar, br), channel(ag, bg), channel(ab, bb))
}

/// A cell's colour when it has one of its own, else `fallback`.
fn rgb_or(colour: Color, fallback: Color) -> Color {
    match colour {
        Color::Rgb(..) => colour,
        _ => fallback,
    }
}

fn secs(t: f32) -> Duration {
    Duration::from_secs_f32(t)
}

/// The earliest moment anything asks to be drawn again.
#[derive(Default)]
struct Wake(Option<Instant>);

impl Wake {
    fn at(&mut self, when: Instant) {
        self.0 = Some(self.0.map_or(when, |w| w.min(when)));
    }

    fn frame(&mut self, now: Instant) {
        self.at(now + FRAME);
    }
}

// ---------------------------------------------------------------------------
// The scripted turn

enum Step {
    User(&'static str),
    Call {
        id: &'static str,
        name: &'static str,
        args: String,
    },
    Result {
        id: &'static str,
        name: &'static str,
        content: &'static str,
    },
    Delegate {
        agent: &'static str,
        task: &'static str,
        session: &'static str,
    },
    Report {
        agent: &'static str,
        session: &'static str,
        report: &'static str,
        failed: bool,
    },
    Usage {
        tokens_in: u32,
        tokens_out: u32,
        context: u32,
    },
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Piece {
    Reasoning,
    Text,
    /// The last piece of a reply has arrived.
    End,
}

/// Steps and streamed pieces, in seconds from the start.
struct Script {
    steps: Vec<(f32, Step)>,
    pieces: Vec<(f32, Piece, String)>,
}

/// A fixed sequence of pseudo-random numbers, so every replay streams the
/// same way.
struct Lcg(u64);

impl Lcg {
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        lo + ((self.0 >> 33) as u32) % (hi - lo + 1)
    }
}

impl Script {
    fn at(&mut self, t: f32, step: Step) {
        self.steps.push((t, step));
    }

    /// Text as a provider streams it: uneven pieces, uneven gaps, and now and
    /// then a stall. Returns when the last piece arrives.
    fn say(&mut self, mut t: f32, text: &str, rng: &mut Lcg) -> f32 {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let end = (i + rng.range(2, 28) as usize).min(chars.len());
            self.pieces
                .push((t, Piece::Text, chars[i..end].iter().collect()));
            i = end;
            let gap = if rng.range(0, 6) == 0 {
                rng.range(300, 520)
            } else {
                rng.range(12, 160)
            };
            t += gap as f32 / 1000.0;
        }
        self.pieces.push((t, Piece::End, String::new()));
        t
    }

    /// Thinking, streamed in steadier pieces.
    fn think(&mut self, mut t: f32, text: &str, rng: &mut Lcg) -> f32 {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let end = (i + rng.range(6, 14) as usize).min(chars.len());
            self.pieces
                .push((t, Piece::Reasoning, chars[i..end].iter().collect()));
            i = end;
            t += rng.range(60, 110) as f32 / 1000.0;
        }
        t
    }
}

const REPORT: &str = "Selesai, ketiga bagian sudah jadi:\n\n\
- Halaman katalog di `src/app/catalogue`, jadi di percobaan kedua setelah build pertama gagal.\n\
- API `GET /api/products` dan `/api/products/:slug`, 14 tes lulus.\n\
- Tabel `products` dengan migrasi dan seed 24 produk.\n\n\
`npm test` lulus 23 tes. Context sudah 88%, sebaiknya dipadatkan dulu sebelum lanjut.";

fn script() -> Script {
    let mut s = Script {
        steps: Vec::new(),
        pieces: Vec::new(),
    };
    let mut rng = Lcg(0x5eed);
    // The message goes out on the first frame: before it, an empty
    // conversation in the sample would ask for a model it does not need.
    let mut t = 0.0;
    s.at(
        t,
        Step::User("buatkan halaman katalog, API produk, dan skemanya sekalian"),
    );
    t = s.think(
        t + 1.4,
        "Tiga area yang bisa jalan bersamaan: halaman katalog, API produk, dan skema. \
         Kontraknya sudah ada di src/shared/types.ts, jadi cukup dibaca lalu dibagi.",
        &mut rng,
    );
    s.at(
        t + 0.2,
        Step::Usage {
            tokens_in: 3_100,
            tokens_out: 420,
            context: 14_800,
        },
    );
    t = s.say(
        t + 0.5,
        "Ini kerja tiga area. Saya baca kontrak tipenya dulu, lalu saya bagi ke tiga agent \
         yang jalan bersamaan.",
        &mut rng,
    );
    t += 0.5;
    s.at(
        t,
        Step::Call {
            id: "t1",
            name: "read",
            args: serde_json::json!({ "path": "src/shared/types.ts" }).to_string(),
        },
    );
    t += 0.9;
    s.at(
        t,
        Step::Result {
            id: "t1",
            name: "read",
            content: "export type Product = { id: string; slug: string; name: string; priceMinor: number };",
        },
    );
    t += 0.5;
    let wave = [
        (
            "fe",
            "Catalogue page in src/app/catalogue: a product grid from GET /api/products.",
            "s-fe",
        ),
        (
            "be",
            "GET /api/products and GET /api/products/:slug, as in src/shared/types.ts.",
            "s-be",
        ),
        (
            "db",
            "A products table with its migration and a seed of 24 products.",
            "s-db",
        ),
    ];
    for (i, (agent, task, session)) in wave.into_iter().enumerate() {
        s.at(
            t + i as f32 * 0.08,
            Step::Delegate {
                agent,
                task,
                session,
            },
        );
    }
    s.at(
        t + 0.5,
        Step::Usage {
            tokens_in: 9_800,
            tokens_out: 1_300,
            context: 38_400,
        },
    );
    s.at(
        t + 2.4,
        Step::Report {
            agent: "db",
            session: "s-db",
            report: "DONE: products table, its migration and a seed of 24 products.\n\
                     CHANGED: db/migrations/0003_products.sql, db/seed.ts\n\
                     VERIFIED: migrated and seeded a fresh database.\n\
                     NEXT: nothing",
            failed: false,
        },
    );
    s.at(
        t + 3.1,
        Step::Usage {
            tokens_in: 21_000,
            tokens_out: 2_900,
            context: 61_500,
        },
    );
    s.at(
        t + 3.8,
        Step::Report {
            agent: "be",
            session: "s-be",
            report: "DONE: GET /api/products and /api/products/:slug with cursor paging.\n\
                     CHANGED: src/server/products.ts, src/server/routes.ts\n\
                     VERIFIED: 14 API tests pass.\n\
                     NEXT: nothing",
            failed: false,
        },
    );
    s.at(
        t + 5.0,
        Step::Report {
            agent: "fe",
            session: "s-fe",
            report: "PARTIAL FAILURE: the page renders, but the build stops on a missing export.\n\
                     CHANGED: src/app/catalogue/page.tsx\n\
                     VERIFIED: not verified, the build fails.\n\
                     NEXT: export ProductCard from src/components/index.ts",
            failed: true,
        },
    );
    t = s.say(
        t + 5.5,
        "Frontend gagal di build. Saya ulang sekali, dengan error-nya di brief.",
        &mut rng,
    );
    t += 0.5;
    s.at(
        t,
        Step::Delegate {
            agent: "fe",
            task: "Finish the catalogue page: the build stops because ProductCard is not exported \
                   from src/components/index.ts.",
            session: "s-fe-2",
        },
    );
    s.at(
        t + 0.6,
        Step::Usage {
            tokens_in: 33_000,
            tokens_out: 4_700,
            context: 83_000,
        },
    );
    t += 2.6;
    s.at(
        t,
        Step::Report {
            agent: "fe",
            session: "s-fe-2",
            report: "DONE: catalogue page with the product grid and paging.\n\
                     CHANGED: src/app/catalogue/page.tsx, src/components/index.ts\n\
                     VERIFIED: builds; checked at 360, 768 and 1440px.\n\
                     NEXT: nothing",
            failed: false,
        },
    );
    t += 0.5;
    s.at(
        t,
        Step::Call {
            id: "t2",
            name: "bash",
            args: serde_json::json!({ "command": "npm test" }).to_string(),
        },
    );
    t += 1.6;
    s.at(
        t,
        Step::Result {
            id: "t2",
            name: "bash",
            content: "Test Files  6 passed (6)\n     Tests  23 passed (23)",
        },
    );
    s.at(
        t + 0.2,
        Step::Usage {
            tokens_in: 47_000,
            tokens_out: 6_800,
            context: 112_600,
        },
    );
    t = s.say(t + 0.6, REPORT, &mut rng);
    s.at(
        t + 0.3,
        Step::Usage {
            tokens_in: 52_400,
            tokens_out: 7_900,
            context: 112_600,
        },
    );
    s.at(t + 0.5, Step::Done);
    s
}

// ---------------------------------------------------------------------------
// Streaming

/// The reply as it reaches the screen. With smoothing on, what the provider
/// sends waits in `backlog` and is let out at a pace that follows how much is
/// waiting, so a burst spreads over the frames after it instead of landing at
/// once, and nothing lags the network by much more than `LAG`.
struct Stream {
    pieces: VecDeque<(Instant, Piece, String)>,
    backlog: VecDeque<char>,
    carry: f32,
    last: Instant,
    /// When each visible character of the reply was shown, newest last.
    shown: VecDeque<Instant>,
    /// A reply has shown text and has not ended.
    replying: bool,
    /// Its last piece has arrived; what waits goes out faster.
    ending: bool,
    /// A reply that ended keeps its caret until its last characters have
    /// faded in.
    caret_until: Option<Instant>,
}

/// How far behind the network smoothing may run.
const LAG: f32 = 0.18;
/// And once the last piece is in.
const LAG_ENDING: f32 = 0.08;
/// The slowest pace, in characters a second.
const MIN_RATE: f32 = 40.0;

impl Stream {
    fn new(script: &Script, epoch: Instant) -> Self {
        Self {
            pieces: script
                .pieces
                .iter()
                .map(|(t, kind, text)| (epoch + secs(*t), *kind, text.clone()))
                .collect(),
            backlog: VecDeque::new(),
            carry: 0.0,
            last: epoch,
            shown: VecDeque::new(),
            replying: false,
            ending: false,
            caret_until: None,
        }
    }

    /// Take what has arrived and show what is due. True when the screen changed.
    fn tick(&mut self, app: &mut TestApp, now: Instant, smooth: bool) -> bool {
        let mut changed = false;
        while self.pieces.front().is_some_and(|(at, ..)| *at <= now) {
            let Some((_, kind, text)) = self.pieces.pop_front() else {
                break;
            };
            match kind {
                Piece::Reasoning => {
                    app.deliver(Event::Reasoning { delta: text });
                    changed = true;
                }
                Piece::Text if smooth => self.backlog.extend(text.chars()),
                Piece::Text => {
                    self.show(app, &text, now, now);
                    changed = true;
                }
                Piece::End => self.ending = true,
            }
        }
        if !self.backlog.is_empty() {
            if smooth {
                let dt = now
                    .saturating_duration_since(self.last)
                    .as_secs_f32()
                    .min(0.1);
                let lag = if self.ending { LAG_ENDING } else { LAG };
                let rate = (self.backlog.len() as f32 / lag).max(MIN_RATE);
                let due = self.carry + rate * dt;
                let n = (due.floor() as usize).min(self.backlog.len());
                self.carry = (due - n as f32).clamp(0.0, 1.0);
                if n > 0 {
                    let text: String = self.backlog.drain(..n).collect();
                    let from = self.last;
                    self.show(app, &text, from, now);
                    changed = true;
                }
            } else {
                // Smoothing was turned off while text waited.
                let text: String = self.backlog.drain(..).collect();
                self.show(app, &text, now, now);
                changed = true;
            }
        }
        if self.ending && self.backlog.is_empty() {
            self.ending = false;
            self.replying = false;
            self.caret_until = self
                .shown
                .back()
                .map(|at| *at + Duration::from_secs_f32((TRAIL_MS + 60.0) / 1000.0));
            changed = true;
        }
        self.last = now;
        changed
    }

    fn show(&mut self, app: &mut TestApp, text: &str, from: Instant, to: Instant) {
        app.deliver(Event::Text {
            delta: text.to_owned(),
        });
        self.replying = true;
        // Spread the characters of one release over the frame it covers, so
        // the trail fades as a slope rather than in steps.
        let visible = text
            .chars()
            .filter(|c| !c.is_whitespace() && !HIDDEN.contains(c))
            .count();
        for i in 1..=visible {
            self.shown
                .push_back(from + (to - from).mul_f32(i as f32 / visible as f32));
        }
        while self.shown.len() > 96 {
            self.shown.pop_front();
        }
    }

    /// Show whatever waits: a new block is about to start.
    fn finish(&mut self, app: &mut TestApp, now: Instant) {
        if !self.backlog.is_empty() {
            let text: String = self.backlog.drain(..).collect();
            self.show(app, &text, now, now);
        }
        self.replying = false;
        self.ending = false;
        self.caret_until = None;
    }

    fn caret(&self, now: Instant) -> bool {
        self.replying || self.caret_until.is_some_and(|until| now < until)
    }

    fn next(&self, now: Instant) -> Option<Instant> {
        let arrival = self.pieces.front().map(|(at, ..)| *at);
        let drain = (!self.backlog.is_empty()).then_some(now + FRAME);
        match (arrival, drain) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

// ---------------------------------------------------------------------------
// What the effects remember

struct ToolFx {
    id: String,
    name: String,
    finished: Option<(Instant, bool)>,
}

struct DelegationFx {
    session: &'static str,
    started: Instant,
    finished: Option<(Instant, bool)>,
}

/// The SESSION card's figures, counting from where they were shown to where
/// the last `Usage` put them.
struct Meters {
    from: [f32; 3],
    to: [f32; 3],
    at: Instant,
}

const COUNT_MS: f32 = 700.0;

impl Meters {
    fn value(&self, now: Instant, full: bool) -> [f32; 3] {
        match progress(self.at, now, COUNT_MS) {
            Some(p) if full => {
                let e = ease_out(p);
                std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * e)
            }
            _ => self.to,
        }
    }

    fn retarget(&mut self, to: [f32; 3], now: Instant) {
        self.from = self.value(now, true);
        self.to = to;
        self.at = now;
    }
}

struct Fx {
    sent: Option<Instant>,
    busy: bool,
    pill: Option<(Color, Instant)>,
    label: String,
    label_at: Instant,
    tools: Vec<ToolFx>,
    delegations: Vec<DelegationFx>,
    meters: Meters,
    percent: f32,
    cross: Option<Instant>,
    tab: Option<(usize, usize, Instant)>,
    palette: Option<(Instant, Buffer)>,
}

impl Fx {
    fn new(now: Instant) -> Self {
        Self {
            sent: None,
            busy: false,
            pill: None,
            label: String::new(),
            label_at: now,
            tools: Vec::new(),
            delegations: Vec::new(),
            meters: Meters {
                from: [0.0; 3],
                to: [0.0; 3],
                at: now,
            },
            percent: 0.0,
            cross: None,
            tab: None,
            palette: None,
        }
    }
}

fn apply(step: &Step, app: &mut TestApp, fx: &mut Fx, now: Instant) {
    match step {
        Step::User(text) => {
            app.push_user(text);
            app.begin_turn();
            app.deliver(Event::MessageStart {
                id: "sample".into(),
            });
            fx.sent = Some(now);
        }
        Step::Call { id, name, args } => {
            app.deliver(Event::ToolCall {
                id: (*id).into(),
                name: (*name).into(),
                arguments: args.clone(),
            });
            fx.tools.push(ToolFx {
                id: (*id).into(),
                name: (*name).into(),
                finished: None,
            });
        }
        Step::Result { id, name, content } => {
            app.deliver(Event::ToolResult {
                id: (*id).into(),
                name: (*name).into(),
                content: (*content).into(),
                is_error: false,
                before: None,
            });
            if let Some(tool) = fx.tools.iter_mut().find(|tool| tool.id == *id) {
                tool.finished = Some((now, false));
            }
        }
        Step::Delegate {
            agent,
            task,
            session,
        } => {
            let id = format!("call-{session}");
            app.deliver(Event::ToolCall {
                id: id.clone(),
                name: "delegate".into(),
                arguments: serde_json::json!({ "agent": agent, "task": task }).to_string(),
            });
            app.deliver(Event::ToolResult {
                id,
                name: "delegate".into(),
                content: format!("delegating to {agent}"),
                is_error: false,
                before: None,
            });
            app.deliver(Event::DelegationStarted {
                agent: (*agent).into(),
                task: (*task).into(),
                session_id: (*session).into(),
            });
            fx.delegations.push(DelegationFx {
                session,
                started: now,
                finished: None,
            });
        }
        Step::Report {
            agent,
            session,
            report,
            failed,
        } => {
            app.deliver(Event::DelegationFinished {
                agent: (*agent).into(),
                summary: (*report).into(),
                session_id: (*session).into(),
                failed: *failed,
            });
            if let Some(d) = fx.delegations.iter_mut().find(|d| d.session == *session) {
                d.finished = Some((now, *failed));
            }
        }
        Step::Usage {
            tokens_in,
            tokens_out,
            context,
        } => {
            app.deliver(Event::Usage {
                input_tokens: *tokens_in,
                output_tokens: *tokens_out,
                context_tokens: *context,
                context_window: WINDOW,
            });
            fx.meters.retarget(
                [*tokens_in as f32, *tokens_out as f32, *context as f32],
                now,
            );
        }
        Step::Done => app.deliver(Event::Done {
            stop_reason: "ready".into(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Finding what to animate in the frame the interface drew

/// The first status marker on a header row.
fn marker_at(buf: &Buffer, rect: Rect) -> Option<u16> {
    (rect.x..rect.right()).find(|&x| MARKS.contains(&buf[(x, rect.y)].symbol()))
}

/// The columns of the transcript an icon can sit in: the border's padding
/// and the icon itself.
const ICON_COLUMNS: u16 = 4;

/// Whether `text` is written from (`x`, `y`) on.
fn text_at(buf: &Buffer, (x, y): (u16, u16), text: &str, right: u16) -> bool {
    (x..)
        .zip(text.chars())
        .all(|(cx, ch)| cx < right && buf[(cx, y)].symbol() == ch.to_string())
}

/// The icon of the newest row for a call to `name`.
fn call_icon(buf: &Buffer, area: Rect, bottom: u16, name: &str) -> Option<(u16, u16)> {
    (area.y..bottom).rev().find_map(|y| {
        (area.x..(area.x + ICON_COLUMNS).min(area.right())).find_map(|x| {
            let symbol = buf[(x, y)].symbol();
            let icon = matches!(symbol, "›" | "✓" | "✗") || SPINNER.contains(&symbol);
            (icon && text_at(buf, (x + 2, y), name, area.right())).then_some((x, y))
        })
    })
}

/// Where `needle` starts on row `y` between `from` and `to`, cell by cell.
fn find_in_row(buf: &Buffer, y: u16, from: u16, to: u16, needle: &str) -> Option<u16> {
    let cells: Vec<(u16, &str)> = (from..to)
        .map(|x| (x, buf[(x, y)].symbol()))
        .filter(|(_, symbol)| !symbol.is_empty())
        .collect();
    let want: Vec<String> = needle.chars().map(String::from).collect();
    (0..cells.len().saturating_sub(want.len() - 1))
        .find(|&i| want.iter().enumerate().all(|(j, ch)| cells[i + j].1 == ch))
        .map(|i| cells[i].0)
}

fn find_caret(buf: &Buffer, area: Rect) -> Option<(u16, u16)> {
    let caret = CARET.to_string();
    (area.y..area.bottom()).rev().find_map(|y| {
        (area.x..area.right())
            .rev()
            .find(|&x| buf[(x, y)].symbol() == caret)
            .map(|x| (x, y))
    })
}

/// Bring a row's text up from its background.
fn fade_in_row(buf: &mut Buffer, y: u16, from: u16, to: u16, t: &Theme, e: f32) {
    for x in from..to {
        let cell = &mut buf[(x, y)];
        let bg = rgb_or(cell.bg, t.panel);
        cell.fg = mix(bg, rgb_or(cell.fg, t.text), 0.15 + 0.85 * e);
    }
}

/// A marker settling into its final colour: from the running yellow to
/// green, or a flash that cools to red.
fn settle(t: &Theme, failed: bool, e: f32) -> Color {
    if failed {
        mix(mix(t.red, t.text, 0.65), t.red, e)
    } else {
        mix(t.yellow, t.green, e)
    }
}

fn spinner(epoch: Instant, now: Instant) -> (&'static str, Instant) {
    let ms = now.saturating_duration_since(epoch).as_millis();
    let step = ms / SPIN_MS;
    let next = epoch + Duration::from_millis(((step + 1) * SPIN_MS) as u64);
    (SPINNER[step as usize % SPINNER.len()], next)
}

// ---------------------------------------------------------------------------
// The sample

struct Sample {
    app: TestApp,
    homes: Vec<PathBuf>,
    ui: Option<Terminal<TestBackend>>,
    last_ui: Option<Buffer>,
    script: Script,
    step: usize,
    stream: Stream,
    fx: Fx,
    epoch: Instant,
    turn_at: Option<Instant>,
    mode: Mode,
    smooth: bool,
}

impl Sample {
    fn new(mode: Mode, smooth: bool) -> Self {
        let mut app = TestApp::new();
        // `TestApp` keeps its settings in a folder of its own; remember it so
        // it can be removed on the way out.
        let homes = std::env::var_os("ENX_HOME")
            .map(PathBuf::from)
            .and_then(|home| home.parent().map(PathBuf::from))
            .into_iter()
            .collect();
        app.begin_conversation();
        app.show_model("claude-sonnet-5-5", WINDOW, 3.0, 15.0);
        let epoch = Instant::now();
        let script = script();
        let stream = Stream::new(&script, epoch);
        Self {
            app,
            homes,
            ui: None,
            last_ui: None,
            script,
            step: 0,
            stream,
            fx: Fx::new(epoch),
            epoch,
            turn_at: None,
            mode,
            smooth,
        }
    }

    fn smoothing(&self) -> bool {
        self.smooth && self.mode == Mode::Full
    }

    /// Play every step and piece that is due. True when the screen changed.
    fn advance(&mut self, now: Instant) -> bool {
        let mut changed = false;
        while let Some((at, step)) = self.script.steps.get(self.step) {
            if self.epoch + secs(*at) > now {
                break;
            }
            self.stream.finish(&mut self.app, now);
            if matches!(step, Step::User(_)) {
                self.turn_at = Some(now);
            }
            apply(step, &mut self.app, &mut self.fx, now);
            self.step += 1;
            changed = true;
        }
        let smooth = self.smoothing();
        changed |= self.stream.tick(&mut self.app, now, smooth);
        changed
    }

    /// The next step, piece or clock tick the interface shows.
    fn next(&self, now: Instant) -> Option<Instant> {
        let mut wake = Wake::default();
        if let Some((at, _)) = self.script.steps.get(self.step) {
            wake.at(self.epoch + secs(*at));
        }
        if let Some(at) = self.stream.next(now) {
            wake.at(at);
        }
        // The turn's clock in the status bar.
        if let (true, Some(turn)) = (self.app.is_busy(), self.turn_at) {
            let elapsed = now.saturating_duration_since(turn).as_secs();
            wake.at(turn + Duration::from_secs(elapsed + 1));
        }
        wake.0
    }

    /// Keep what the effects follow in step with the interface.
    fn observe(&mut self, now: Instant) {
        let full = self.mode == Mode::Full;
        let t = self.app.theme();
        let [tokens_in, tokens_out, context] = self.fx.meters.value(now, full);
        self.app
            .set_meters(tokens_in as u32, tokens_out as u32, context as u32);
        let percent = 100.0 * context / WINDOW as f32;
        if self.fx.percent < 85.0 && percent >= 85.0 && full {
            self.fx.cross = Some(now);
        }
        self.fx.percent = percent;
        let busy = self.app.is_busy();
        if busy != self.fx.busy {
            let from = if self.fx.busy { t.yellow } else { t.green };
            self.fx.pill = Some((from, now));
            self.fx.busy = busy;
        }
        let label = self.app.activity_label();
        if label != self.fx.label {
            self.fx.label = label;
            self.fx.label_at = now;
        }
    }

    fn draw(
        &mut self,
        term: &mut Terminal<CrosstermBackend<Counted>>,
        hud: &Hud,
        now: Instant,
    ) -> io::Result<Option<Instant>> {
        let size = term.size()?;
        let ui_area = Rect::new(0, 0, size.width, size.height.saturating_sub(1));
        if self
            .ui
            .as_ref()
            .is_none_or(|ui| ui.backend().buffer().area != ui_area)
        {
            self.ui = Some(Terminal::new(TestBackend::new(
                ui_area.width,
                ui_area.height,
            ))?);
        }
        self.observe(now);
        let caret = self.smoothing() && self.stream.caret(now);
        if caret {
            if let Some(text) = self.app.last_text_mut() {
                text.push(CARET);
            }
        }
        let Some(ui) = self.ui.as_mut() else {
            return Ok(None);
        };
        let app = &mut self.app;
        ui.draw(|frame| app.draw(frame))?;
        if caret {
            if let Some(text) = self.app.last_text_mut() {
                if text.ends_with(CARET) {
                    text.pop();
                }
            }
        }
        let drawn = ui.backend().buffer().clone();
        let mut wake = None;
        execute!(term.backend_mut(), BeginSynchronizedUpdate)?;
        term.draw(|frame| {
            let buf = frame.buffer_mut();
            buf.merge(&drawn);
            wake = self.composite(buf, ui_area, now);
            hud.render(
                buf,
                Rect::new(0, ui_area.bottom(), size.width, 1),
                &self.app.theme(),
                self,
            );
        })?;
        execute!(term.backend_mut(), EndSynchronizedUpdate)?;
        self.last_ui = Some(drawn);
        Ok(wake)
    }

    /// The motion, laid over the frame the interface drew. Returns when it
    /// next needs a frame.
    fn composite(&self, buf: &mut Buffer, ui: Rect, now: Instant) -> Option<Instant> {
        if self.mode == Mode::Off || ui.height < 2 {
            return None;
        }
        let full = self.mode == Mode::Full;
        let t = self.app.theme();
        let mut wake = Wake::default();

        // Calls in the transcript: a spinner while one runs, and its mark
        // settling into colour once it is done. Found by the icon, because
        // a call with nothing to open records no header.
        if let Some(area) = self.app.transcript_rect() {
            let bottom = area.bottom().min(ui.bottom());
            for y in area.y..bottom {
                for x in area.x..(area.x + ICON_COLUMNS).min(area.right()) {
                    let cell = &mut buf[(x, y)];
                    if cell.symbol() == "›" && cell.fg == t.yellow {
                        let (frame, next) = spinner(self.epoch, now);
                        cell.set_symbol(frame);
                        wake.at(next);
                    }
                }
            }
            if full {
                for tool in &self.fx.tools {
                    let Some((at, failed)) = tool.finished else {
                        continue;
                    };
                    let Some(p) = progress(at, now, 360.0) else {
                        continue;
                    };
                    if let Some((x, y)) = call_icon(buf, area, bottom, &tool.name) {
                        buf[(x, y)].fg = settle(&t, failed, ease_out(p));
                        wake.frame(now);
                    }
                }
            }
        }
        // Delegation rows in the transcript, which do record their header.
        for (rect, id) in self.app.tool_header_marks() {
            if !full || rect.y >= ui.bottom() {
                continue;
            }
            let Some(d) = self.fx.delegations.iter().find(|d| d.session == id) else {
                continue;
            };
            if let Some(x) = marker_at(buf, rect) {
                self.delegation_marker(buf, (x, rect.y), d, &t, now, &mut wake);
            }
            if let Some(p) = progress(d.started, now, 320.0) {
                fade_in_row(buf, rect.y, rect.x, rect.right(), &t, ease_out(p));
                wake.frame(now);
            }
        }
        if !full {
            return wake.0;
        }

        // The message just sent: a wash of the accent that drains away.
        if let Some(p) = self.fx.sent.and_then(|at| progress(at, now, 650.0)) {
            let rows = self.app.user_block_rects();
            let newest = rows.iter().map(|(_, index)| *index).max();
            let strength = 0.28 * (1.0 - ease_out(p));
            for (rect, _) in rows.iter().filter(|(_, index)| Some(*index) == newest) {
                for y in rect.y..rect.bottom().min(ui.bottom()) {
                    for x in rect.x..rect.right() {
                        let cell = &mut buf[(x, y)];
                        cell.bg = mix(rgb_or(cell.bg, t.panel), t.accent, strength);
                    }
                }
            }
            wake.frame(now);
        }

        // The reply: a caret where it writes, the newest characters fading in.
        if self.smooth {
            if let Some(area) = self.app.transcript_rect() {
                if let Some((cx, cy)) = find_caret(buf, area) {
                    buf[(cx, cy)].fg = t.accent;
                    let (mut x, mut y) = (cx, cy);
                    let mut k = 0;
                    loop {
                        if x == area.x {
                            if y == area.y || cy - y >= 3 {
                                break;
                            }
                            y -= 1;
                            x = area.right() - 1;
                        } else {
                            x -= 1;
                        }
                        let cell = &mut buf[(x, y)];
                        if cell.symbol().trim().is_empty() {
                            continue;
                        }
                        let Some(&shown) = self.stream.shown.iter().rev().nth(k) else {
                            break;
                        };
                        k += 1;
                        let Some(p) = progress(shown, now, TRAIL_MS) else {
                            break;
                        };
                        let bg = rgb_or(cell.bg, t.panel);
                        cell.fg = mix(bg, rgb_or(cell.fg, t.text), 0.2 + 0.8 * ease_out(p));
                        wake.frame(now);
                    }
                    if self.stream.caret(now) {
                        wake.frame(now);
                    }
                }
            }
        }

        // The sidebar.
        if let Some(side) = self.app.sidebar_rect() {
            let inner_right = side.right().saturating_sub(2);
            for (rect, index) in self.app.delegation_marks() {
                let Some(d) = self.fx.delegations.get(index) else {
                    continue;
                };
                if rect.y >= ui.bottom() {
                    continue;
                }
                if let Some(x) = marker_at(buf, rect) {
                    self.delegation_marker(buf, (x, rect.y), d, &t, now, &mut wake);
                }
                if let Some(p) = progress(d.started, now, 320.0) {
                    for y in rect.y..(rect.y + rect.height.max(2)).min(ui.bottom()) {
                        fade_in_row(buf, y, rect.x, inner_right, &t, ease_out(p));
                    }
                    wake.frame(now);
                }
            }
            // The context bar turning red eases into it.
            if let Some(p) = self.fx.cross.and_then(|at| progress(at, now, 520.0)) {
                let rows = side.y..side.bottom().min(ui.bottom());
                if let Some(y) = rows
                    .into_iter()
                    .find(|&y| find_in_row(buf, y, side.x, side.right(), "context").is_some())
                {
                    for x in side.x..side.right() {
                        let cell = &mut buf[(x, y)];
                        if cell.fg == t.red {
                            cell.fg = mix(t.accent, t.red, ease_out(p));
                        }
                    }
                }
                wake.frame(now);
            }
            // A tab change: the highlight passes from one name to the other.
            if let Some((from, to, at)) = self.fx.tab {
                if let Some(p) = progress(at, now, 220.0) {
                    let e = ease_out(p);
                    for (rect, index) in self.app.sidebar_tab_rects() {
                        let colour = if index == to {
                            mix(t.muted, t.accent, e)
                        } else if index == from {
                            mix(t.accent, t.muted, e)
                        } else {
                            continue;
                        };
                        for x in rect.x..rect.right() {
                            buf[(x, rect.y)].fg = colour;
                        }
                    }
                    wake.frame(now);
                }
            }
        }

        // The status bar: the state pill changes colour, the activity label
        // fades in when it changes and shimmers while the model is quiet.
        let status_y = ui.bottom() - 1;
        if let Some((from, at)) = self.fx.pill {
            if let Some(p) = progress(at, now, 320.0) {
                let to = if self.app.is_busy() {
                    t.yellow
                } else {
                    t.green
                };
                for x in ui.x..ui.right() {
                    let cell = &mut buf[(x, status_y)];
                    if cell.bg == to {
                        cell.bg = mix(from, to, ease_out(p));
                    }
                }
                wake.frame(now);
            }
        }
        if self.app.is_busy() {
            let label = &self.fx.label;
            let needle = format!("· {label}");
            if let Some(x0) = find_in_row(buf, status_y, ui.x, ui.right(), &needle) {
                let x0 = x0 + 2;
                let n = label.chars().count() as u16;
                let fade = progress(self.fx.label_at, now, 240.0).map(ease_out);
                let shimmer = matches!(label.as_str(), "waiting for the model" | "thinking");
                let period = 1.8;
                let phase = now
                    .saturating_duration_since(self.fx.label_at)
                    .as_secs_f32()
                    % period
                    / period;
                let centre = -4.0 + phase * (n as f32 + 8.0);
                for i in 0..n.min(ui.right().saturating_sub(x0)) {
                    let cell = &mut buf[(x0 + i, status_y)];
                    let mut colour = rgb_or(cell.fg, t.muted);
                    if shimmer {
                        let glow = (1.0 - (i as f32 - centre).abs() / 3.5).max(0.0);
                        colour = mix(colour, t.text, glow * 0.85);
                    }
                    if let Some(e) = fade {
                        colour = mix(rgb_or(cell.bg, t.canvas), colour, e);
                    }
                    cell.fg = colour;
                }
                if fade.is_some() {
                    wake.frame(now);
                } else if shimmer {
                    wake.at(now + SHIMMER_FRAME);
                }
            }
        }

        // The command list grows up out of the composer.
        if let (Some((at, behind)), Some((px, py, pw, ph))) =
            (&self.fx.palette, self.app.inline_palette_area())
        {
            if let Some(p) = progress(*at, now, 150.0) {
                let shown = (ph as f32 * ease_out(p)).ceil() as u16;
                for y in py..(py + ph).saturating_sub(shown) {
                    for x in px..(px + pw).min(behind.area.right()) {
                        if y < behind.area.bottom() {
                            buf[(x, y)] = behind[(x, y)].clone();
                        }
                    }
                }
                wake.frame(now);
            }
        }
        wake.0
    }

    /// A delegation's marker: breathing while it runs, then settling.
    fn delegation_marker(
        &self,
        buf: &mut Buffer,
        (x, y): (u16, u16),
        d: &DelegationFx,
        t: &Theme,
        now: Instant,
        wake: &mut Wake,
    ) {
        let cell = &mut buf[(x, y)];
        match d.finished {
            None => {
                let beat = now.saturating_duration_since(self.epoch).as_secs_f32() / 1.8 * TAU;
                let k = 0.5 + 0.5 * beat.cos();
                cell.fg = mix(mix(t.yellow, t.panel, 0.6), t.yellow, k);
                wake.at(now + SLOW_FRAME);
            }
            Some((at, failed)) => {
                if let Some(p) = progress(at, now, 360.0) {
                    cell.fg = settle(t, failed, ease_out(p));
                    wake.frame(now);
                }
            }
        }
    }

    /// A key. Returns true to quit.
    fn key(&mut self, code: KeyCode, ctrl: bool, now: Instant) -> bool {
        if ctrl && code == KeyCode::Char('c') {
            return true;
        }
        if self.app.input_text().starts_with('/') {
            // The command list is open. Enter would run the command, and the
            // sample has nothing to run.
            if code != KeyCode::Enter {
                let _ = self.app.press_key(code);
            }
            if !self.app.input_text().starts_with('/') {
                self.fx.palette = None;
            }
            return false;
        }
        match code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('r') => {
                let old = std::mem::take(&mut self.homes);
                *self = Sample::new(self.mode, self.smooth);
                remove_homes(&old);
            }
            KeyCode::Char('m') => self.mode = self.mode.next(),
            KeyCode::Char('s') => self.smooth = !self.smooth,
            KeyCode::Char(digit @ '1'..='4') => {
                let to = digit as usize - '1' as usize;
                let from = self.app.sidebar_tab();
                if from != to {
                    self.app.select_sidebar_tab(to);
                    self.fx.tab = Some((from, to, now));
                }
            }
            KeyCode::Char('/') => {
                let behind = self.last_ui.clone();
                let _ = self.app.press_key(KeyCode::Char('/'));
                if self.app.input_text().starts_with('/') {
                    self.fx.palette = behind.map(|buf| (now, buf));
                }
            }
            _ => {}
        }
        false
    }
}

/// Remove the folders `TestApp` made, only ever its own.
fn remove_homes(homes: &[PathBuf]) {
    for home in homes {
        let own = home
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("enx-test-"));
        if own && home.starts_with(std::env::temp_dir()) {
            let _ = std::fs::remove_dir_all(home);
        }
    }
}

// ---------------------------------------------------------------------------
// The bottom row: what the motion costs

struct Hud {
    window: Instant,
    frames: u32,
    bytes: u64,
    shown: (u32, u64),
}

impl Hud {
    fn count(&mut self, now: Instant, bytes: u64) {
        if now >= self.window + Duration::from_secs(2) {
            // The first frame after a quiet spell starts a fresh second.
            self.window = now;
            self.frames = 0;
            self.bytes = 0;
        }
        self.frames += 1;
        self.bytes += bytes;
    }

    /// Close the second when it is over. True when the figures shown change.
    fn roll(&mut self, now: Instant) -> bool {
        if now < self.window + Duration::from_secs(1) {
            return false;
        }
        let fresh = (self.frames, self.bytes);
        self.window = now;
        self.frames = 0;
        self.bytes = 0;
        let changed = fresh != self.shown;
        self.shown = fresh;
        changed
    }

    fn next(&self) -> Option<Instant> {
        (self.frames > 0 || self.shown != (0, 0)).then_some(self.window + Duration::from_secs(1))
    }

    fn render(&self, buf: &mut Buffer, area: Rect, t: &Theme, sample: &Sample) {
        let (frames, bytes) = self.shown;
        let rate = if bytes >= 1024 {
            format!("{:.1} KB/s", bytes as f32 / 1024.0)
        } else {
            format!("{bytes} B/s")
        };
        let muted = Style::default().fg(t.muted);
        let text = Style::default().fg(t.text);
        let mut spans = vec![
            Span::styled(
                " motion sample ",
                Style::default().fg(t.panel).bg(t.accent2),
            ),
            Span::styled("  motion ", muted),
            Span::styled(sample.mode.label(), text),
            Span::styled(" · smooth ", muted),
            Span::styled(if sample.smooth { "on" } else { "off" }, text),
            Span::styled("   ", muted),
            Span::styled(format!("{frames} frames/s"), text),
            Span::styled(" · ", muted),
            Span::styled(rate, text),
        ];
        if area.width < 100 {
            spans.push(Span::styled(
                "   widen to 100 columns for the sidebar",
                Style::default().fg(t.red),
            ));
        }
        let left = Line::from(spans);
        let used = left.width() as u16;
        Paragraph::new(left)
            .style(Style::default().bg(t.canvas))
            .render(area, buf);
        let keys = "r replay  m motion  s smooth  / commands  1-4 tabs  q quit ";
        let width = keys.chars().count() as u16;
        if area.width > used + width + 2 {
            Paragraph::new(Line::styled(keys, muted))
                .render(Rect::new(area.right() - width, area.y, width, 1), buf);
        }
    }
}

/// Stdout, counting what goes through it.
struct Counted {
    out: io::Stdout,
    bytes: Rc<Cell<u64>>,
}

impl Write for Counted {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let n = self.out.write(data)?;
        self.bytes.set(self.bytes.get() + n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

fn restore() {
    let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
    let _ = disable_raw_mode();
}

fn main() -> io::Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        default_hook(info);
    }));
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)?;
    let result = run();
    restore();
    result
}

fn run() -> io::Result<()> {
    let bytes = Rc::new(Cell::new(0u64));
    let mut term = Terminal::new(CrosstermBackend::new(Counted {
        out: io::stdout(),
        bytes: bytes.clone(),
    }))?;
    term.clear()?;
    let mut sample = Sample::new(Mode::Full, true);
    let mut hud = Hud {
        window: Instant::now(),
        frames: 0,
        bytes: 0,
        shown: (0, 0),
    };
    let mut dirty = true;
    let mut wake: Option<Instant> = None;
    let result = loop {
        let now = Instant::now();
        let hud_changed = hud.roll(now);
        let changed = sample.advance(now);
        if dirty || changed || wake.is_some_and(|at| now >= at) {
            let before = bytes.get();
            wake = sample.draw(&mut term, &hud, now)?;
            hud.count(now, bytes.get() - before);
            dirty = false;
        } else if hud_changed {
            // Only the figures changed: not counted as a frame of motion.
            sample.draw(&mut term, &hud, now)?;
        }
        let mut next = Wake::default();
        for at in [wake, sample.next(now), hud.next()].into_iter().flatten() {
            next.at(at);
        }
        let timeout = next
            .0
            .map(|at| at.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(60));
        if event::poll(timeout)? {
            match event::read()? {
                Input::Key(key) if key.kind != KeyEventKind::Release => {
                    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                    if sample.key(key.code, ctrl, Instant::now()) {
                        break Ok(());
                    }
                    dirty = true;
                }
                Input::Resize(..) => dirty = true,
                _ => {}
            }
        }
    };
    let homes = std::mem::take(&mut sample.homes);
    drop(sample);
    remove_homes(&homes);
    result
}
