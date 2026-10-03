//! The footer and `/model` agree with the active agent's model. Setting an
//! agent to another model used to leave the footer on the old one until the
//! next `/model`; and picking a model while an agent had its own changed the
//! conversation's model, not the agent's, so they drifted apart.

use enowx_tui::testing::TestApp;

const W: u16 = 160;
const H: u16 = 40;

fn seed(app: &mut TestApp) {
    // The built-in test provider is at 127.0.0.1:1, so test/* runs; a second
    // local provider lets a second model run too, both without a key. Seeding
    // test/chat here also makes it the conversation's model.
    app.seed_provider("test", "http://127.0.0.1:1", "", "", "chat");
    app.seed_provider("lab", "http://127.0.0.1:2", "", "", "");
}

/// The footer names the active agent's own model as soon as it is set, with
/// no `/model` in between.
#[test]
fn the_footer_follows_the_agents_model() {
    let mut app = TestApp::in_conversation();
    seed(&mut app);
    assert!(
        app.footer_model().starts_with("test/chat"),
        "{}",
        app.footer_model()
    );

    // The orchestrator is the active agent in a fresh conversation.
    app.set_agent_model("orchestrator", "lab/planner");
    assert_eq!(
        app.footer_model(),
        "lab/planner",
        "the footer updates at once"
    );
    let bar = app.status_bar(W, H);
    assert!(bar.contains("lab/planner"), "{bar}");
}

/// Picking a model while the active agent has its own changes that agent's
/// model, so the footer and the agent stay in step.
#[test]
fn picking_a_model_updates_the_active_agents_own() {
    let mut app = TestApp::in_conversation();
    seed(&mut app);
    app.set_agent_model("orchestrator", "lab/planner");

    app.pick_model("test/other").unwrap();
    assert_eq!(
        app.agent_own_model("orchestrator").as_deref(),
        Some("test/other"),
        "the agent's own model changed, not just the conversation's"
    );
    assert_eq!(app.footer_model(), "test/other");
    // The conversation's shared model is left alone.
    assert_eq!(app.active_model(), "test/chat");
}
