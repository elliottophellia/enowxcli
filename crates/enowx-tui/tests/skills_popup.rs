//! `/skills`: what each skill is, and for a built-in one, which agents carry
//! it, since only they are offered it.

use enowx_tui::testing::TestApp;

#[test]
fn a_builtin_skill_says_which_agents_carry_it() {
    let mut app = TestApp::new();
    app.enter_skills_modal();
    let rows = app.render_to_text(120, 30);
    let ui = rows
        .iter()
        .find(|row| row.contains(" ui ") && row.contains("built-in"))
        .unwrap_or_else(|| panic!("the ui row: {rows:#?}"));
    assert!(ui.contains("built-in for fe, mobile, review"), "{ui}");
    let writing = rows
        .iter()
        .find(|row| row.contains(" writing ") && row.contains("built-in"))
        .unwrap_or_else(|| panic!("the writing row: {rows:#?}"));
    assert!(
        writing.contains("for docs, fe, general, review"),
        "{writing}"
    );
}
