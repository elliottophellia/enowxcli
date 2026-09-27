//! Render the whole app at any terminal size, as plain text, to check the
//! grid without a terminal. Scenarios: session (default), empty, busy,
//! palette, detail, rich, delegate, work.
//! Run: cargo run -q -p enowx-tui --example snapshot -- 120 36 rich
use enowx_tui::testing::TestApp;

fn session() -> TestApp {
    let mut app = TestApp::new();
    app.push_user("perbagus sidebar dan rapikan layout transcript");
    app.switch_agent("fe", "UI work in the terminal interface");
    app.push_assistant(
        "Saya cek dulu struktur sidebar dan bagian yang merender tab.\n\n\
         Ada **tiga** hal yang perlu dirapikan:\n\n\
         1. Lebar kolom label tidak konsisten antar tab\n\
         2. Pemisah memakai garis yang nyaris tak terlihat\n\
         3. Footer menimpa sudut kotak sidebar\n\n\
         ```rust\nfn sidebar_width(total: u16) -> u16 {\n    (total * 2 / 5).clamp(38, 60)\n}\n```",
    );
    app.push_tool(
        "t1",
        "read",
        r#"{"path":"crates/enowx-tui/src/ui/sidebar.rs"}"#,
        "use super::*;\n\npub(super) fn draw_sidebar(...) {\n    ...\n}\n",
    );
    app.push_tool(
        "t2",
        "grep",
        r#"{"pattern":"draw_split_line","path":"crates"}"#,
        "crates/enowx-tui/src/ui/chrome.rs:316\ncrates/enowx-tui/src/ui/chrome.rs:144\n",
    );
    app.push_tool(
        "t3",
        "bash",
        r#"{"command":"cargo test -p enowx-tui"}"#,
        "running 304 tests\ntest result: ok. 304 passed; 0 failed\n",
    );
    app.push_assistant(
        "Semua test lulus. Sidebar sekarang memakai grid dua kolom dengan lebar label tetap.",
    );
    app
}

/// States beyond the plain session, selected by the third argument.
fn scenario(name: &str) -> TestApp {
    match name {
        "empty" => TestApp::new(),
        "busy" => {
            let mut app = session();
            app.set_busy(true);
            app.type_input("tambahkan juga test untuk layout sempit");
            app
        }
        "palette" => {
            let mut app = session();
            app.type_input("/m");
            app
        }
        "detail" => {
            let mut app = session();
            app.expand_tool("t3");
            app.deliver_delegation_started("fe", "rapikan sidebar", "s-1");
            app.deliver_delegation_finished("fe", "s-1", false);
            app.deliver_delegation_started("review", "cek hasil layout", "s-2");
            app.push_error("provider returned 429: rate limited");
            app
        }
        "rich" => {
            let mut app = TestApp::new();
            app.set_show_reasoning(true);
            app.set_show_tool_output(true);
            app.push_user("ringkas perubahan layout dan tunjukkan diff-nya");
            app.push_reasoning("The user wants a summary plus the diff. Check chrome.rs first.");
            app.push_assistant(
                "## Ringkasan\n\n\
                 Layout sekarang memakai **grid** dengan `Grid::new` sebagai satu-satunya\n\
                 sumber posisi. Lihat [catatan](https://example.com).\n\n\
                 > Kotak tidak lagi punya bingkai luar.\n\n\
                 | Bagian | Lebar | Catatan |\n|---|---|---|\n| chat | sisa | fleksibel |\n| side | 40 / 52 | bertingkat |\n\n\
                 - poin pertama\n- poin kedua dengan `kode`\n",
            );
            app.push_tool(
                "e1",
                "edit",
                r#"{"path":"src/ui/chrome.rs","old_text":"let side = 38;\nlet gap = 2;","new_text":"let side = 40;\nlet gap = 1;"}"#,
                "edited at line 10",
            );
            app.push_tool(
                "w1",
                "write",
                r##"{"path":"docs/grid.md","content":"# Grid\n\nOne edge for text."}"##,
                "wrote docs/grid.md",
            );
            app.push_tool(
                "td",
                "todo",
                r#"{"items":[{"state":"done","label":"frame and columns"},{"state":"in_progress","label":"chat content"},{"state":"pending","label":"popups"}]}"#,
                "",
            );
            app.push_retry("provider returned 503", 2, 10);
            app.push_assistant("Selesai. Semua kotak sejajar.");
            app
        }
        "delegate" => {
            let mut app = TestApp::new();
            app.push_user("buatkan portfolio simple");
            app.push_assistant("Ini kerja frontend, saya serahkan ke fe.");
            app.push_tool(
                "d1",
                "delegate",
                r#"{"agent":"fe","task":"Build a simple portfolio page"}"#,
                "delegating to fe",
            );
            app.deliver_delegation_started(
                "fe",
                "Build a simple one-page portfolio.\n- hero with name and role\n- projects grid\n- contact links",
                "s-1",
            );
            app.deliver_delegation_report(
                "fe",
                "s-1",
                "DONE: Built a one-page portfolio with a hero, a projects grid and contact links.\n\
                 CHANGED: index.html, style.css\n\
                 VERIFIED: Opened it at 375 and 1280 wide; no horizontal scroll.\n\
                 NEXT: nothing",
                false,
            );
            app.push_assistant("Portfolio sudah jadi: `index.html` dan `style.css`.");
            app
        }
        "work" => {
            let mut app = TestApp::new();
            app.push_user("halo, bisa tolong buatkan saya portofolio sederhana");
            app.push_reasoning(
                "Workspace is empty. Plan: index.html + styles.css, no build step.\n\n\
                 Placeholders in Indonesian, since the user speaks Indonesian.",
            );
            app.push_assistant("Workspace kosong, saya buat dari nol.");
            let html = "<!DOCTYPE html>\n<html lang=\"id\">\n<head>\n  <meta charset=\"utf-8\">\n  \
                        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  \
                        <title>Rani Prasetyo - Frontend Developer</title>\n  \
                        <!-- styles live in one file -->\n  \
                        <link rel=\"stylesheet\" href=\"styles.css\">\n</head>\n<body>\n  \
                        <a class=\"skip-link\" href=\"#main\">Lewati ke konten utama</a>\n"
                .to_owned()
                + &"  <p>...</p>\n".repeat(139);
            app.push_tool(
                "w1",
                "write",
                &serde_json::json!({"path": "index.html", "content": html}).to_string(),
                "wrote index.html",
            );
            app.push_reasoning("Now styles.css. Simple, restrained palette, light/dark.");
            let css = "/* ==== Token ==== */\n:root {\n  --kertas: #fbfaf8;\n  --tinta: #1a1a18;\n  \
                       --aksen: #1f5f4a;\n  --jarak: clamp(1rem, 4vw, 2rem);\n  --radius: 10px;\n}\n"
                .to_owned()
                + &"body { margin: 0; }\n".repeat(244);
            app.push_tool(
                "w2",
                "write",
                &serde_json::json!({"path": "styles.css", "content": css}).to_string(),
                "wrote styles.css",
            );
            app.expand_tool("w2");
            app.push_reasoning(
                "Verify: classes in HTML exist in CSS. Reading both back is cheapest.",
            );
            app.push_tool("r1", "read", r#"{"path":"index.html"}"#, "…");
            app.push_assistant(
                "Selesai. Portofolio satu halaman:\n\n- `index.html`: hero, about, projects\n- `styles.css`: light/dark, responsif",
            );
            app
        }
        _ => session(),
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let nums: Vec<u16> = raw.iter().filter_map(|a| a.parse().ok()).collect();
    let (w, h) = (
        nums.first().copied().unwrap_or(120),
        nums.get(1).copied().unwrap_or(36),
    );
    let name = raw
        .iter()
        .find(|a| a.parse::<u16>().is_err())
        .map(String::as_str)
        .unwrap_or("session");
    let mut app = scenario(name);
    for line in app.render_to_text(w, h) {
        println!("{line}");
    }
}
