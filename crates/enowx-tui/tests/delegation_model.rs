//! Each delegated sub-agent shows the model it runs on in the sidebar list,
//! so a glance shows what is answering each one.

use enowx_tui::testing::TestApp;

const W: u16 = 160;
const H: u16 = 50;

fn side(app: &mut TestApp) -> String {
    app.side_column(W, H).join("\n")
}

#[test]
fn the_list_names_each_sub_agents_model() {
    let mut app = TestApp::in_conversation();
    // The test provider (127.0.0.1:1) is the conversation's model; lab is a
    // second local provider for fe's own model.
    app.seed_provider("test", "http://127.0.0.1:1", "", "", "chat");
    app.seed_provider("lab", "http://127.0.0.1:2", "", "", "");
    // fe runs on a model of its own; be falls back to the conversation's.
    app.set_agent_model("fe", "lab/design-pro");

    app.deliver_delegation_started("fe", "build the page", "branch-fe");
    app.deliver_delegation_started("be", "wire the API", "branch-be");

    let screen = side(&mut app);
    assert!(
        screen.contains("design-pro"),
        "fe's own model is shown: {screen}"
    );
    assert!(
        screen.contains("chat"),
        "be shows the conversation's model: {screen}"
    );
}
