//! The enowX mark: an X of lit cells on a 5 by 5 grid, its centre cell in
//! the brand's orange, the other cells of the grid faint. The home screen
//! draws it large beside the letters `enow`; a running turn draws it small
//! above the composer. Both move the same way: the centre lights and stays,
//! and the rings around it light outward, hold, and go out inward, round
//! after round.

use ratatui::style::Color;

use crate::theme::Theme;

pub(super) const CELLS: usize = 5;

/// When the parts of one round happen, as fractions of the round.
const CENTRE_ON: f32 = 0.04;
const RING_STAGGER: f32 = 0.05;
const RING_FADE_IN: f32 = 0.06;
const RINGS_OUT: f32 = 0.75;
const RING_FADE_OUT: f32 = 0.075;
/// How much of the text colour a cell of the grid keeps while it is not lit.
const GRID: f32 = 0.12;

/// The colour of the cell at `col`, `row`, `t` seconds after the mark first
/// appeared, with rounds `round` seconds long.
pub(super) fn cell_colour(theme: &Theme, col: usize, row: usize, t: f32, round: f32) -> Color {
    let grid = mix(theme.canvas, theme.text, GRID);
    if row != col && row != CELLS - 1 - col {
        return grid;
    }
    let ring = row.abs_diff(CELLS / 2).max(col.abs_diff(CELLS / 2));
    if ring == 0 {
        let lit = ease_out((t / round - CENTRE_ON) / RING_FADE_IN);
        return mix(grid, crate::theme::BRAND, lit);
    }
    mix(grid, theme.text, ring_lit(ring, (t % round) / round))
}

/// How lit a ring is, from 0 to 1, a fraction `at` into a round: on from the
/// centre outward, off from the outside inward.
pub(super) fn ring_lit(ring: usize, at: f32) -> f32 {
    let on = CENTRE_ON + ring as f32 * RING_STAGGER;
    let off = RINGS_OUT + (CELLS / 2 - ring) as f32 * RING_STAGGER;
    ease_out((at - on) / RING_FADE_IN) * (1.0 - ease_out((at - off) / RING_FADE_OUT))
}

pub(super) fn ease_out(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    1.0 - (1.0 - p).powi(3)
}

/// `from` moved `amount` of the way to `to`. A theme without RGB colours
/// has nothing to interpolate, so it snaps at the halfway point.
pub(super) fn mix(from: Color, to: Color, amount: f32) -> Color {
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

/// Draw pixels two to a character cell, the upper one in the foreground of
/// `▀` and the lower in its background. `pixel(x, y)` is None where nothing
/// is drawn.
pub(super) fn draw_pixels(
    buf: &mut ratatui::buffer::Buffer,
    theme: &Theme,
    at: ratatui::layout::Rect,
    pixel: impl Fn(usize, usize) -> Option<Color>,
) {
    let area = buf.area;
    for row in 0..at.height {
        for col in 0..at.width {
            let (x, y) = (at.x + col, at.y + row);
            if x >= area.right() || y >= area.bottom() {
                continue;
            }
            let top = pixel(col as usize, row as usize * 2);
            let bottom = pixel(col as usize, row as usize * 2 + 1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rings_light_outward_and_go_out_inward_every_round() {
        assert_eq!(ring_lit(1, 0.0), 0.0);
        assert!(
            ring_lit(1, 0.15) > ring_lit(2, 0.15),
            "the inner ring first"
        );
        assert_eq!(ring_lit(2, 0.5), 1.0, "both held");
        assert!(
            ring_lit(2, 0.84) < ring_lit(1, 0.84),
            "the outer ring out first"
        );
        assert_eq!(ring_lit(1, 0.98), 0.0, "dark before the next round");
    }
}
