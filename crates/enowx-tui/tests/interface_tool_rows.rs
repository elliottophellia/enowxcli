//! The interface tools read as rows that say what they looked at and what
//! they found, not a bare tool name and a line count.

use enowx_tui::testing::TestApp;

fn row(name: &str, args: &str, result: &str) -> String {
    let mut app = TestApp::in_conversation();
    app.push_user("cek tampilannya");
    app.push_tool("t1", name, args, result);
    app.render_to_text(120, 40).join("\n")
}

#[test]
fn a_preview_row_names_the_page_and_its_problems() {
    let screen = row(
        "preview",
        r#"{"path":"site/index.html"}"#,
        "Preview of site/index.html\n\n360px, page 900px tall: 2 problems\n  touch targets under 44px:\n    - a.tiny 29x28px\n\n768px, page 900px tall: 1 problem\n\n1440px, page 900px tall: nothing found\n",
    );
    let header = screen
        .lines()
        .find(|line| line.contains("preview"))
        .unwrap_or_default();
    assert!(header.contains("site/index.html"), "{screen}");
    assert!(header.contains("3 problems"), "{screen}");
}

#[test]
fn a_clean_preview_says_so() {
    let screen = row(
        "preview",
        r#"{"url":"http://localhost:5173/","start":"npm run dev"}"#,
        "Preview of http://localhost:5173/\n\n360px, page 900px tall: nothing found\n",
    );
    let header = screen
        .lines()
        .find(|line| line.contains("preview"))
        .unwrap_or_default();
    assert!(header.contains("localhost:5173"), "{screen}");
    assert!(header.contains("nothing found"), "{screen}");
}

#[test]
fn ui_check_and_icon_rows_say_what_they_looked_at() {
    let screen = row(
        "ui_check",
        r#"{"path":"src"}"#,
        "12 findings in 9 interface files: 2 high, 6 medium, 4 low.\n1. HIGH ...",
    );
    let header = screen
        .lines()
        .find(|line| line.contains("ui_check"))
        .unwrap_or_default();
    assert!(
        header.contains("src") && header.contains("12 findings"),
        "{screen}"
    );

    let screen = row(
        "icon",
        r#"{"action":"search","query":"clock","set":"tabler"}"#,
        "tabler:clock\ntabler:clock-hour-4",
    );
    assert!(screen.contains("tabler: clock"), "{screen}");
}

#[test]
fn an_ask_row_shows_the_first_of_several_questions() {
    let screen = row(
        "ask",
        r#"{"questions":[{"question":"Untuk siapa aplikasinya?","options":[{"label":"Tim"}]},{"question":"Tema?","options":[{"label":"Gelap"}]}]}"#,
        "The user answered:\n1. Untuk siapa aplikasinya?\n   Chose: Tim.",
    );
    let header = screen
        .lines()
        .find(|line| line.contains("ask"))
        .unwrap_or_default();
    assert!(header.contains("Untuk siapa aplikasinya?"), "{screen}");
    assert!(header.contains("2 questions"), "{screen}");
}
