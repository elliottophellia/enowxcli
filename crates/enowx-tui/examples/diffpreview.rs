//! Render an `edit` diff so the gutter, line numbers and +/- colouring can be
//! inspected outside a session. Run: cargo run -p enowx-tui --example diffpreview
use enowx_tui::preview_diff;

fn main() {
    let old = r#"pub fn highlight(line: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    for c in line.chars() {
        out.push(Tok::Plain);
    }
    out
}"#;
    let new = r#"pub fn highlight(line: &str, syntax: &Syntax, state: &mut State) -> Vec<Tok> {
    let mut out = Vec::new();
    for c in line.chars() {
        if state.in_block_comment {
            out.push(Tok::Comment);
            continue;
        }
        out.push(Tok::Plain);
    }
    merge_adjacent(out)
}"#;
    for line in preview_diff("crates/enowx-tui/src/syntax.rs", old, new, 172, 74) {
        println!("{line}");
    }
}
