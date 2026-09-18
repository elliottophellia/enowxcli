//! Measure the cost of drawing a frame as a session grows, with the render
//! cache warm versus cold. Run: cargo run --release -p enowx-tui --example benchcache
use enowx_tui::testing::TestApp;
use std::time::Instant;

fn main() {
    for blocks in [20usize, 100, 400] {
        let mut app = TestApp::new();
        for i in 0..blocks {
            app.push_user(&format!("question number {i}"));
            app.push_assistant(&format!(
                "Answer {i} with some **bold** text and `code`.\n\n```rust\nfn f{i}() -> usize {{ {i} }}\n```\n\n- point one\n- point two\n"
            ));
        }
        let _ = app.render_to_text(80, 24);

        // Warm: one token arrives, only the last block is dirty.
        let start = Instant::now();
        for _ in 0..50 {
            app.append_to_last(" more");
            let _ = app.render_to_text(80, 24);
        }
        let warm = start.elapsed();

        // Cold: the whole transcript is re-parsed every frame (the old behaviour).
        let start = Instant::now();
        for _ in 0..50 {
            app.append_to_last(" more");
            app.clear_render_cache();
            let _ = app.render_to_text(80, 24);
        }
        let cold = start.elapsed();

        println!(
            "{:>4} blocks | cached {:>8.2?}/frame | uncached {:>8.2?}/frame | {:.1}x",
            blocks * 2,
            warm / 50,
            cold / 50,
            cold.as_secs_f64() / warm.as_secs_f64()
        );
    }
}
