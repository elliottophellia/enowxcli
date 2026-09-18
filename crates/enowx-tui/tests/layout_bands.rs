//! Where two panes meet, a background that stops at the divider shows up as a
//! block of the wrong colour. The composer's raised band has to continue
//! behind the sidebar so the strip reads as one surface.

use enowx_tui::testing::TestApp;

const W: u16 = 120;
const H: u16 = 20;

/// Background runs of a row, as (hex, count).
fn runs(app: &mut TestApp, row: u16) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for bg in app.row_backgrounds(W, H, row) {
        match out.last_mut() {
            Some((c, n)) if *c == bg => *n += 1,
            _ => out.push((bg, 1)),
        }
    }
    out
}

/// The interior of the composer row must be one colour on both sides of the
/// divider — only the frame and the divider itself may differ.
#[test]
fn the_composer_band_continues_past_the_divider() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    // The row holding the prompt marker.
    let rows = app.render_to_text(W, H);
    let prompt = rows
        .iter()
        .position(|r| r.contains('❯'))
        .expect("the prompt row");

    let runs = runs(&mut app, prompt as u16);
    // Expected: frame, divider-ish edge, band, divider, band, edge, frame.
    // What matters is that the two wide runs share a colour.
    let wide: Vec<&(String, usize)> = runs.iter().filter(|(_, n)| *n > 10).collect();
    assert!(
        wide.len() >= 2,
        "expected a band on both sides of the divider, got {runs:?}"
    );
    assert_eq!(
        wide[0].0, wide[1].0,
        "the band must be one colour across the divider, got {runs:?}"
    );
}

/// The sidebar's pager row sits inside the band; painting the band must not
/// erase it.
#[test]
fn the_sidebar_pager_survives_the_band() {
    let mut app = TestApp::new();
    for i in 0..60 {
        app.push_assistant(&format!("line {i}"));
    }
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains("page")),
        "the pager should still be readable, got {rows:?}"
    );
}

/// Without a sidebar there is no seam to fix, and the band must not spill
/// outside the frame.
#[test]
fn a_narrow_layout_keeps_its_band_inside_the_frame() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = app.render_to_text(70, H);
    for row in &rows {
        assert!(
            row.starts_with("  │") || row.starts_with("  ╭") || row.starts_with("  ╰") || row.trim().is_empty(),
            "a row escaped the frame: {row:?}"
        );
    }
}
