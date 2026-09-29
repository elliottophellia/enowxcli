//! The home screen: what a new conversation opens on.
//!
//! No sidebar and no transcript box. Until the first message there is
//! nothing for them to show, so the window holds one block in its middle:
//! the wordmark, the composer, and under it the state, agent and model the
//! status bar shows everywhere else, with where the agent will work (or what
//! to set up before it can) and the version. The status bar keeps only its
//! keys, on the columns they have in the chat layout.
//!
//! The block sits a little above the middle, and the composer takes three
//! quarters of the window's width.

use super::*;
use ratatui::style::Color;
use std::time::Instant;

/// The letters `enow`, one character per pixel, with strokes two pixels
/// thick each way. The mark stands in for the X after them.
const LETTERS: [&str; 10] = [
    ".######...#######....######..##......##",
    "########..########..########.##......##",
    "##....##..##....##..##....##.##......##",
    "##....##..##....##..##....##.##..##..##",
    "########..##....##..##....##.##..##..##",
    "########..##....##..##....##.##..##..##",
    "##........##....##..##....##.##..##..##",
    "##........##....##..##....##.##..##..##",
    "########..##....##..########.##########",
    ".#######..##....##...######...###..###.",
];
const LETTERS_W: usize = 39;

/// The mark's cells are 2 by 2 pixels with a pixel between them: 14 pixels
/// square, standing four pixels above the letters' tops, as a capital does.
const CELL: usize = 2;
const PITCH: usize = CELL + 1;
const MARK_PX: usize = mark::CELLS * PITCH - 1;
const MARK_GAP: usize = 3;
const LOGO_W: u16 = (LETTERS_W + MARK_GAP + MARK_PX) as u16;
/// A character cell holds two pixel rows.
const LOGO_H: u16 = (MARK_PX / 2) as u16;

/// The composer's narrowest, and its widest, so a message on a very wide
/// window still wraps at a measure that can be read back.
const HOME_MIN_W: u16 = 64;
const HOME_MAX_W: u16 = 150;
/// What the empty composer says it is for.
const PLACEHOLDER: &str = "Ask, or type / for commands";

/// One round of the mark's motion, in seconds.
const ROUND: f32 = 4.0;
/// The letters fade in as the mark first lights, a beat apart.
const LETTER_FADE: f32 = 0.6;
const LETTER_STAGGER: f32 = 0.05;

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
    let box_w = home_width(body);
    let info = info_rows(app, inner(box_w));
    let info_h = info.len() as u16;

    let field_w = box_w.saturating_sub(COMPOSER_LEFT + 1 + PAD_X).max(1) as usize;
    let (input, row, col) = input_rows(&app.input, app.cursor, field_w);
    let ih = (input.len().clamp(1, 8) as u16 + FRAME).min((body.height / 2).max(FRAME + 1));
    // Placed for a one-line composer. One that grows with the message grows
    // down, so the mark stays put, and moves up only when it would pass
    // the bottom.
    // A little above the middle: two parts of the free rows above, three
    // below, so what sits under the composer is not pressed to the keys.
    let top = (body.y + body.height.saturating_sub(block(info_h)) * 2 / 5)
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

/// The composer's width: three quarters of the window, kept between its
/// narrowest and widest and never closer than two columns to either edge,
/// and even with the window's width so the two sides get the same columns.
fn home_width(body: Rect) -> u16 {
    let widest = body.width.saturating_sub(4).min(HOME_MAX_W);
    let width = (body.width * 3 / 4).clamp(HOME_MIN_W.min(widest), widest);
    if (body.width - width) % 2 == 1 {
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

fn draw_wordmark(frame: &mut Frame, theme: &Theme, at: Rect, elapsed: f32) {
    mark::draw_pixels(frame.buffer_mut(), theme, at, |x, y| {
        pixel(theme, x, y, elapsed)
    });
}

/// One pixel of the wordmark at `elapsed` seconds, or None where it is empty.
fn pixel(theme: &Theme, x: usize, y: usize, elapsed: f32) -> Option<Color> {
    if x < LETTERS_W {
        let row = y.checked_sub(MARK_PX - LETTERS.len())?;
        if LETTERS[row].as_bytes()[x] != b'#' {
            return None;
        }
        let letter = (x / 10) as f32;
        let shown = mark::ease_out((elapsed - letter * LETTER_STAGGER) / LETTER_FADE);
        return (shown > 0.0).then(|| mark::mix(theme.canvas, theme.text, shown));
    }
    let x = x.checked_sub(LETTERS_W + MARK_GAP)?;
    if x % PITCH == CELL || y % PITCH == CELL {
        return None;
    }
    Some(mark::cell_colour(
        theme,
        x / PITCH,
        y / PITCH,
        elapsed,
        ROUND,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_letters_are_one_rectangle_beside_the_mark() {
        for row in LETTERS {
            assert_eq!(row.len(), LETTERS_W, "{row}");
        }
        assert_eq!((LOGO_W, LOGO_H), (56, 7));
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        assert_eq!(tail("/a/very/long/path/to/shop", 10), "…h/to/shop");
        assert_eq!(tail("/short", 10), "/short");
    }
}
