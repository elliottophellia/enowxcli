//! The home screen: what a new conversation opens on.
//!
//! No sidebar and no transcript box. Until the first message there is
//! nothing for them to show, so the window holds one block in its middle:
//! the mark, the composer, and under it the state, agent and model the
//! status bar shows everywhere else, with where the agent will work (or what
//! to set up before it can) and the version. The status bar keeps only its
//! keys, on the columns they have in the chat layout.
//!
//! The block is centred with the same gap on every side as it looks on
//! screen, which is what sets the composer's width.

use super::*;
use ratatui::style::Color;
use std::time::Instant;

/// The enowX mark: an X of lit cells on a 5 by 5 grid, its centre cell in
/// the brand colour. Cells are 2 by 2 pixels with a pixel between them, and a
/// character cell holds two pixel rows, so the mark is 14 columns by 7 rows.
const CELLS: usize = 5;
const CELL: usize = 2;
const PITCH: usize = CELL + 1;
const LOGO_W: u16 = (CELLS * PITCH - 1) as u16;
const LOGO_H: u16 = LOGO_W / 2;

/// The composer's narrowest, and its widest, so a message on a very wide
/// window still wraps at a measure that can be read back.
const HOME_MIN_W: u16 = 57;
const HOME_MAX_W: u16 = 120;
/// What the empty composer says it is for.
const PLACEHOLDER: &str = "Ask, or type / for commands";

/// The mark's motion, in seconds, repeated for as long as the screen is up.
/// The centre lights first and stays; the rings around it light outward, hold,
/// and go out inward, and the next round lights them again.
const ROUND: f32 = 4.0;
const CENTRE_ON: f32 = 0.15;
const RING_STAGGER: f32 = 0.2;
const RING_FADE_IN: f32 = 0.22;
const RINGS_OUT: f32 = 3.0;
const RING_FADE_OUT: f32 = 0.3;
/// How much of the text colour a cell of the grid keeps while it is not lit.
const GRID: f32 = 0.12;

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
    // The mark goes first when space runs out; the composer never does.
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
    // down, so the mark stays put, and moves up only when it would pass
    // the bottom.
    let top = (body.y + body.height.saturating_sub(block(info_h)) / 2)
        .min(body.bottom().saturating_sub(logo_rows + ih + info_h))
        .max(body.y);

    if show_logo {
        let at = Rect::new(body.x + (body.width - LOGO_W) / 2, top, LOGO_W, LOGO_H);
        draw_mark(frame, &app.theme, at, elapsed);
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
        // mark only when the window is too short for that.
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
    } else if app.config.has_connected_provider() {
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

fn draw_mark(frame: &mut Frame, theme: &Theme, at: Rect, elapsed: f32) {
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

/// One pixel of the mark at `elapsed` seconds, or None between cells.
fn pixel(theme: &Theme, x: usize, y: usize, elapsed: f32) -> Option<Color> {
    if x % PITCH == CELL || y % PITCH == CELL {
        return None;
    }
    let (col, row) = (x / PITCH, y / PITCH);
    let grid = mix(theme.canvas, theme.text, GRID);
    if row != col && row != CELLS - 1 - col {
        return Some(grid);
    }
    let ring = row.abs_diff(CELLS / 2).max(col.abs_diff(CELLS / 2));
    if ring == 0 {
        let lit = ease_out((elapsed - CENTRE_ON) / RING_FADE_IN);
        return Some(mix(grid, crate::theme::BRAND, lit));
    }
    Some(mix(grid, theme.text, ring_lit(ring, elapsed % ROUND)))
}

/// How lit a ring is, from 0 to 1, `t` seconds into a round: on from the
/// centre outward, off from the outside inward.
fn ring_lit(ring: usize, t: f32) -> f32 {
    let on = CENTRE_ON + ring as f32 * RING_STAGGER;
    let off = RINGS_OUT + (CELLS / 2 - ring) as f32 * RING_STAGGER;
    ease_out((t - on) / RING_FADE_IN) * (1.0 - ease_out((t - off) / RING_FADE_OUT))
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
    fn the_rings_light_outward_and_go_out_inward_every_round() {
        assert_eq!((LOGO_W, LOGO_H), (14, 7));
        assert_eq!(ring_lit(1, 0.0), 0.0);
        assert!(ring_lit(1, 0.6) > ring_lit(2, 0.6), "the inner ring first");
        assert_eq!(ring_lit(2, 2.0), 1.0, "both held");
        assert!(
            ring_lit(2, 3.35) < ring_lit(1, 3.35),
            "the outer ring out first"
        );
        assert_eq!(ring_lit(1, 3.9), 0.0, "dark before the next round");
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        assert_eq!(tail("/a/very/long/path/to/shop", 10), "…h/to/shop");
        assert_eq!(tail("/short", 10), "/short");
    }
}
