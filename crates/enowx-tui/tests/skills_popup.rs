//! `/skills`: what each skill is, and for a built-in one, which agents carry
//! it, since only they are offered it.

use enowx_tui::testing::TestApp;

#[test]
fn a_builtin_skill_says_which_agents_carry_it() {
    let mut app = TestApp::new();
    app.enter_skills_modal();
    let rows = app.render_to_text(140, 90);
    let ui = rows
        .iter()
        .find(|row| row.contains(" ui ") && row.contains("built-in"))
        .unwrap_or_else(|| panic!("the ui row: {rows:#?}"));
    assert!(
        ui.contains("built-in for canvas, fe, general, mobile, motion"),
        "{ui}"
    );
    let writing = rows
        .iter()
        .find(|row| row.contains(" writing ") && row.contains("built-in"))
        .unwrap_or_else(|| panic!("the writing row: {rows:#?}"));
    assert!(
        writing.contains("for canvas, docs, fe, general"),
        "{writing}"
    );
}

/// The built-in parts (`ui-layout`, `ui-part-hero`...) are listed through
/// `ui`, which says how many it carries.
#[test]
fn built_in_parts_are_listed_through_their_parent() {
    let parts = enowx_core::discovery::skills::builtin_names()
        .filter(|name| name.starts_with("ui-"))
        .count();
    let mut app = TestApp::new();
    app.enter_skills_modal();
    let popup = app.render_to_text(140, 90).join("\n");
    assert!(!popup.contains("ui-part-hero"), "{popup}");
    assert!(!popup.contains("ui-layout"), "{popup}");
    assert!(popup.contains(&format!("ui +{parts} parts")), "{popup}");

    let mut app = TestApp::in_conversation();
    app.select_sidebar_tab(2);
    let side = app.side_column(140, 60).join("\n");
    assert!(side.contains("BUILT-IN"), "{side}");
    assert!(!side.contains("ui-part"), "{side}");
    assert!(side.contains(&format!("ui +{parts} parts")), "{side}");
    assert!(side.contains("brainstorm"), "{side}");
}
