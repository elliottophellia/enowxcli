//! The home screen: what a new conversation opens on.
//!
//! No sidebar and no transcript box. Until the first message there is
//! nothing for them to show, so the window holds one block in its middle:
//! the wordmark, the composer, and under it the state, agent and model the
//! status bar shows everywhere else, with where the agent will work (or what
//! to set up before it can) and the version. The status bar keeps only its
//! keys, on the columns they have in the chat layout.
//!
//! The block is centred with the same gap on every side as it looks on
//! screen, which is what sets the composer's width.

use super::*;
use ratatui::style::Color;
use std::time::Instant;

/// The enowX wordmark, one character per pixel: `#` a letter, `b` the X's
/// long stroke, `s` its short one, `o` the dot where they cross.
///
/// Drawn from the brand wordmark (`assets/brand/logos`) at two-pixel strokes,
/// the weight the letters have at this size. The SVG's own paths rasterised
/// at this size filled the whole X: its strokes are a third of its width.
const WORDMARK: [&str; 10] = [
    "...............................bb......ss",
    "................................bb....ss.",
    ".................................bb..ss..",
    "..................................bbss...",
    ".####..#####...####..##....##......oo....",
    "##..##.##..##.##..##.##....##......oo....",
    "######.##..##.##..##.##.##.##.....ssbb...",
    "##.....##..##.##..##.##.##.##....ss..bb..",
    "##..##.##..##.##..##.########...ss....bb.",
    ".####..##..##..####...##..##...ss......bb",
];

/// The wordmark's size on screen: a character cell holds two pixel rows.
const LOGO_W: u16 = 41;
const LOGO_H: u16 = 5;

/// The composer's narrowest: eight columns past the wordmark on each side.
const HOME_MIN_W: u16 = LOGO_W + 16;
/// And its widest, so a message on a very wide window still wraps at a
/// measure that can be read back.
const HOME_MAX_W: u16 = 120;
/// What the empty composer says it is for.
const PLACEHOLDER: &str = "Ask, or type / for commands";

/// The opening, in seconds from the moment the screen went up. The X's long
/// stroke draws first, top to bottom; the short one follows from both ends
/// towards the middle; then the dot where they cross lights up. The letters
/// fade in over the same stretch, a beat apart.
const LONG_STROKE_ROW: f32 = 0.035;
const SHORT_STROKE_FROM: f32 = 0.30;
const SHORT_STROKE_ROW: f32 = 0.075;
const DOT_ON: f32 = 0.60;
const DOT_LIT: f32 = 0.80;
const LETTER_FADE: f32 = 0.60;
const LETTER_STAGGER: f32 = 0.05;
/// After the opening only the dot moves: one slow breath, so the screen
/// reads as ready without anything competing with the composer.
const BREATH: f32 = 2.4;

pub(super) fn draw_home(frame: &mut Frame, app: &mut App, area: Rect) {
    let elapsed = app
        .home_started
        .get_or_insert_with(Instant::now)
        .elapsed()
        .as_secs_f32();
    app.forget_transcript_targets();
    app.scroll = 0;
    app.max_scroll = 0;

    let status = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    draw_keys(frame, app, status);

    const FRAME: u16 = 2;
    // The wordmark goes first when space runs out; the composer never does.
    let show_logo = body.width >= LOGO_W + 2 && body.height >= LOGO_H + FRAME + 8;
    let logo_rows = if show_logo { LOGO_H + 1 } else { 0 };
    // Sized for a one-line composer, and for the line under it; a second
    // line when the composer is too narrow to carry everything on one.
    let block = |info: u16| logo_rows + FRAME + 1 + info;
    let inner = |width: u16| width.saturating_sub(2 * (1 + PAD_X));
    let mut box_w = home_width(body, block(1));
    let mut info = info_rows(app, inner(box_w));
    if info.len() > 1 {
        box_w = home_width(body, block(info.len() as u16));
        info = info_rows(app, inner(box_w));
    }
    let info_h = info.len() as u16;

    let field_w = box_w.saturating_sub(COMPOSER_LEFT + 1 + PAD_X).max(1) as usize;
    let (input, row, col) = input_rows(&app.input, app.cursor, field_w);
    let ih = (input.len().clamp(1, 8) as u16 + FRAME).min((body.height / 2).max(FRAME + 1));
    // Centred for a one-line composer. One that grows with the message grows
    // down, so the wordmark stays put, and moves up only when it would pass
    // the bottom.
    let top = (body.y + body.height.saturating_sub(block(info_h)) / 2)
        .min(body.bottom().saturating_sub(logo_rows + ih + info_h))
        .max(body.y);

    if show_logo {
        let at = Rect::new(body.x + (body.width - LOGO_W) / 2, top, LOGO_W, LOGO_H);
        draw_wordmark(frame, &app.theme, at, elapsed);
    }
    let boxed = Rect::new(
        body.x + body.width.saturating_sub(box_w) / 2,
        top + logo_rows,
        box_w,
        ih.min(body.bottom().saturating_sub(top + logo_rows)),
    );
    let look = ComposerLook {
        fill: app.theme.canvas,
        placeholder: Some(PLACEHOLDER),
    };
    draw_composer_box(frame, app, boxed, &input, row, col, look);

    let below = body.bottom().saturating_sub(boxed.bottom());
    let matches = app.command_matches();
    if !matches.is_empty() {
        // Under the composer, in place of the lines it covers, and over the
        // wordmark only when the window is too short for that.
        let wanted = matches.len().min(10) as u16 + FRAME;
        let above = boxed.y.saturating_sub(body.y);
        if below > FRAME {
            let rect = Rect::new(boxed.x, boxed.bottom(), boxed.width, wanted.min(below));
            draw_palette(frame, app, rect, &matches);
        } else if above > FRAME {
            let h = wanted.min(above);
            let rect = Rect::new(boxed.x, boxed.y - h, boxed.width, h);
            draw_palette(frame, app, rect, &matches);
        }
    } else {
        // On the composer's own text columns, so the lines read as belonging
        // to the field above them: the state badge starts under the `❯`.
        let inset = 1 + PAD_X;
        for (offset, (left, right)) in (0..below).zip(info) {
            let line = Rect::new(
                boxed.x + inset,
                boxed.bottom() + offset,
                inner(boxed.width),
                1,
            );
            draw_split_line(frame, left, right, line);
        }
    }
}

/// The composer's width for a block `block_h` rows tall, centred with the
/// same gap on every side as it looks on screen. A cell is about twice as
/// tall as it is wide, so the gap at the sides is twice as many columns as
/// the gap above and below is rows. Kept even with the window's width, so
/// the two sides get the same number of columns.
fn home_width(body: Rect, block_h: u16) -> u16 {
    let gap_rows = body.height.saturating_sub(block_h) / 2;
    let widest = body
        .width
        .saturating_sub(4)
        .max(body.width.min(12))
        .min(HOME_MAX_W);
    let narrowest = HOME_MIN_W.min(widest);
    let width = body
        .width
        .saturating_sub(4 * gap_rows)
        .clamp(narrowest, widest);
    if (body.width - width) % 2 == 1 && width > narrowest {
        width - 1
    } else {
        width
    }
}

/// The lines under the composer, each a left and a right half: the state,
/// agent and model on the left; where the work will happen, or what to set
/// up, and the version on the right. One line when the composer is wide
/// enough to carry both, and the state on a line of its own when not.
fn info_rows(app: &App, width: u16) -> Vec<(Line<'static>, Line<'static>)> {
    let t = app.theme;
    let width = width as usize;
    let status = Line::from(status_spans(app));
    let version = Span::styled(
        format!("v{}", env!("CARGO_PKG_VERSION")),
        Style::default().fg(t.faint),
    );
    let setup = if app.config.is_ready() {
        None
    } else if app.config.provider_active() && app.config.model.default.is_empty() {
        Some(("Choose a model", "/model"))
    } else {
        Some(("Connect a provider", "/provider"))
    };
    let place = home_relative(&app.workspace);
    let context = |room: usize| -> Vec<Span<'static>> {
        match setup {
            Some((what, command)) => vec![
                Span::styled(
                    what,
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" · {command}"), Style::default().fg(t.muted)),
            ],
            None => vec![Span::styled(
                tail(&place, room),
                Style::default().fg(t.muted),
            )],
        }
    };
    // Between the halves, and between the place and the version.
    const GAP: usize = 3;
    const SPACE: usize = 2;
    let room = width.saturating_sub(status.width() + GAP + version.width() + SPACE);
    let context_w: usize = context(usize::MAX).iter().map(Span::width).sum();
    // The path gives way first: one line while enough of it shows to say
    // which project this is.
    let one_line = context_w <= room || (setup.is_none() && room >= 16);
    if one_line {
        let mut right = context(room);
        right.push(Span::raw(" ".repeat(SPACE)));
        right.push(version);
        vec![(status, Line::from(right))]
    } else {
        let room = width.saturating_sub(version.width() + SPACE);
        vec![
            (status, Line::default()),
            (Line::from(context(room)), Line::from(vec![version])),
        ]
    }
}

/// The path with the home directory as `~`.
fn home_relative(path: &std::path::Path) -> String {
    let shown = path.display().to_string();
    match std::env::var_os("HOME").map(std::path::PathBuf::from) {
        Some(home) if !home.as_os_str().is_empty() => match path.strip_prefix(&home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => shown,
        },
        _ => shown,
    }
}

/// The end of `text` in `width` columns. A path is read from its end: the
/// project's own directory is the part that says where this is.
fn tail(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let keep = &chars[chars.len() - (width - 1)..];
    format!("…{}", keep.iter().collect::<String>())
}

fn draw_wordmark(frame: &mut Frame, theme: &Theme, at: Rect, elapsed: f32) {
    let buf = frame.buffer_mut();
    let area = buf.area;
    for row in 0..LOGO_H {
        for col in 0..LOGO_W {
            let (x, y) = (at.x + col, at.y + row);
            if x >= area.right() || y >= area.bottom() {
                continue;
            }
            let top = pixel(theme, col as usize, row as usize * 2, elapsed);
            let bottom = pixel(theme, col as usize, row as usize * 2 + 1, elapsed);
            let (symbol, fg, bg) = match (top, bottom) {
                (None, None) => continue,
                (Some(a), None) => ("▀", a, theme.canvas),
                (None, Some(b)) => ("▄", b, theme.canvas),
                (Some(a), Some(b)) if a == b => ("█", a, theme.canvas),
                (Some(a), Some(b)) => ("▀", a, b),
            };
            buf[(x, y)].set_symbol(symbol).set_fg(fg).set_bg(bg);
        }
    }
}

/// One pixel of the wordmark at `elapsed` seconds into the opening, or None
/// while it has not appeared.
fn pixel(theme: &Theme, x: usize, y: usize, elapsed: f32) -> Option<Color> {
    match WORDMARK[y].as_bytes()[x] {
        b'#' => {
            let letter = (x / 7).min(3) as f32;
            let shown = ease_out((elapsed - letter * LETTER_STAGGER) / LETTER_FADE);
            (shown > 0.0).then(|| mix(theme.canvas, theme.text, shown))
        }
        b'b' => long_stroke(theme, y, elapsed),
        b's' => {
            let from_end = y.min(WORDMARK.len() - 1 - y) as f32;
            let shade = (y / 2).min(LOGO_H as usize - 1 - y / 2) as f32;
            (elapsed >= SHORT_STROKE_FROM + from_end * SHORT_STROKE_ROW)
                .then(|| mix(theme.accent2, theme.accent, 0.25 * shade / 2.0))
        }
        b'o' => {
            // Until it lights, the dot is the long stroke it sits on.
            if elapsed < DOT_ON {
                return long_stroke(theme, y, elapsed);
            }
            // One colour for both of its pixel rows: it is a single mark.
            let stroke = long_stroke_colour(theme, DOT_ROW);
            if elapsed < DOT_LIT {
                return Some(mix(
                    stroke,
                    theme.text,
                    (elapsed - DOT_ON) / (DOT_LIT - DOT_ON),
                ));
            }
            let breath = 0.5 + 0.5 * (std::f32::consts::TAU * (elapsed - DOT_LIT) / BREATH).cos();
            Some(mix(stroke, theme.text, 0.35 + 0.65 * breath))
        }
        _ => None,
    }
}

fn long_stroke(theme: &Theme, y: usize, elapsed: f32) -> Option<Color> {
    (elapsed >= y as f32 * LONG_STROKE_ROW).then(|| long_stroke_colour(theme, y))
}

/// The pixel row the dot starts on.
const DOT_ROW: usize = 4;

/// Towards the second accent as it descends, the way the brand's stroke
/// runs from sky to indigo. Stepped per character row, not per pixel row,
/// so a cell's two halves share a colour and draw as one block.
fn long_stroke_colour(theme: &Theme, y: usize) -> Color {
    mix(
        theme.accent,
        theme.accent2,
        0.35 * (y / 2) as f32 / (LOGO_H - 1) as f32,
    )
}

fn ease_out(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    1.0 - (1.0 - p).powi(3)
}

/// `from` moved `amount` of the way to `to`. A theme without RGB colours
/// has nothing to interpolate, so it snaps at the halfway point.
fn mix(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
            Color::Rgb(lerp(r1, r2), lerp(g1, g2), lerp(b1, b2))
        }
        _ if amount < 0.5 => from,
        _ => to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wordmark_is_one_rectangle() {
        for row in WORDMARK {
            assert_eq!(row.len(), LOGO_W as usize, "{row}");
        }
        assert_eq!(WORDMARK.len(), 2 * LOGO_H as usize);
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        assert_eq!(tail("/a/very/long/path/to/shop", 10), "…h/to/shop");
        assert_eq!(tail("/short", 10), "/short");
    }
}
