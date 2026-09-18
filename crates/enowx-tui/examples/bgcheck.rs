//! Print the background colour runs of the last few rows, to find seams.
//! Run: cargo run -p enowx-tui --example bgcheck
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let (w, h) = (120u16, 20u16);
    for row in (h - 5)..h {
        let bgs = app.row_backgrounds(w, h, row);
        let mut runs: Vec<(String, usize)> = Vec::new();
        for bg in bgs {
            match runs.last_mut() {
                Some((c, n)) if *c == bg => *n += 1,
                _ => runs.push((bg, 1)),
            }
        }
        let shown: Vec<String> = runs.iter().map(|(c, n)| format!("{c}×{n}")).collect();
        println!("row {row:>2}: {}", shown.join("  "));
    }
}
