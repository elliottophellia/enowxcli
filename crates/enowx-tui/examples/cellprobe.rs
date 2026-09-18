//! Print the exact cells around the divider on the footer row.
//! Run: cargo run -p enowx-tui --example cellprobe
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let (w, h) = (120u16, 20u16);
    let rows = app.render_to_text(w, h);
    let footer = rows
        .iter()
        .position(|r| r.contains("Orchestrator"))
        .expect("footer row") as u16;
    // The divider column, from a body row.
    let body = rows
        .iter()
        .position(|r| r.matches('│').count() >= 3)
        .unwrap();
    let col = rows[body]
        .char_indices()
        .filter(|(_, c)| *c == '│')
        .map(|(i, _)| rows[body][..i].chars().count())
        .nth(1)
        .unwrap() as u16;

    for row in [footer - 1, footer, footer + 1] {
        let bgs = app.row_backgrounds(w, h, row);
        let text = app.row_text(w, h, row);
        let ch: Vec<char> = text.chars().collect();
        let slice: Vec<String> = ((col - 2)..=(col + 2))
            .map(|x| {
                format!(
                    "{}:{}{}",
                    x,
                    ch.get(x as usize).copied().unwrap_or(' '),
                    format_args!("[{}]", bgs[x as usize])
                )
            })
            .collect();
        println!("row {row:>2} (divider at {col}): {}", slice.join("  "));
    }
}
