//! Cost of a transcript full of tool calls, collapsed versus expanded, and
//! what the raw results occupy in memory.
//! Run: cargo run --release -p enowx-tui --example benchtools
use enowx_tui::testing::TestApp;
use std::time::Instant;

fn main() {
    // A grep across a repo, a glob over a tree: the results that prompted the
    // question about weight.
    let grep_hits: String = (0..400)
        .map(|i| format!("crates/enowx-core/src/provider.rs:{i}:    fn handler_{i}()\n"))
        .collect();
    let glob_paths: String = (0..300)
        .map(|i| format!("crates/enowx-tui/src/ui/module_{i}.rs\n"))
        .collect();
    let bash_out: String = (0..800).map(|i| format!("line {i} of output\n")).collect();

    for calls in [20usize, 100, 400] {
        let mut app = TestApp::new();
        let mut bytes = 0usize;
        for i in 0..calls {
            let (tool, args, result) = match i % 3 {
                0 => ("grep", r#"{"pattern":"fn handler"}"#, grep_hits.as_str()),
                1 => ("glob", r#"{"pattern":"**/*.rs"}"#, glob_paths.as_str()),
                _ => ("bash", r#"{"command":"cargo test"}"#, bash_out.as_str()),
            };
            bytes += result.len();
            app.push_tool(&format!("t{i}"), tool, args, result);
        }
        let _ = app.render_to_text(100, 40);

        let start = Instant::now();
        for _ in 0..30 {
            let _ = app.render_to_text(100, 40);
        }
        let warm = start.elapsed() / 30;

        let start = Instant::now();
        for _ in 0..30 {
            app.clear_render_cache();
            let _ = app.render_to_text(100, 40);
        }
        let cold = start.elapsed() / 30;

        println!(
            "{calls:>4} calls | results held {:>7.1} MB | frame cached {:>8.2?} | uncached {:>8.2?}",
            bytes as f64 / 1_048_576.0,
            warm,
            cold
        );
    }
}
