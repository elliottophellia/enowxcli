//! The agent roster, the handover markers, and the footer's agent name.
//!
//! A handover that is not shown is the failure these guard against: the reply
//! changes voice and the user reads it as the model misbehaving. So every
//! assertion here is about something being VISIBLE on screen, not about state.

use enowx_tui::testing::TestApp;

/// Wide and tall enough for the sidebar to appear at all.
const W: u16 = 120;
const H: u16 = 40;

/// The Agents tab.
const AGENT_TAB: usize = 0;

fn screen(app: &mut TestApp) -> Vec<String> {
    app.render_to_text(W, H)
}

/// Rows of the side column's Agents tab, paged through so an entry below the
/// fold still counts as shown.
fn agent_tab_rows(app: &mut TestApp) -> Vec<String> {
    app.select_sidebar_tab(AGENT_TAB);
    let mut rows = app.side_column(W, H);
    for _ in 0..8 {
        app.page_sidebar_next();
        rows.extend(app.side_column(W, H));
    }
    rows
}

#[test]
fn the_agent_tab_lists_the_roster() {
    let mut app = TestApp::new();
    let roster = app.roster_names();
    assert!(
        roster.len() > 3,
        "the shipped roster should carry more than the three legacy roles: {roster:?}"
    );
    let rows = agent_tab_rows(&mut app);

    for name in ["orchestrator", "fe", "be", "review"] {
        assert!(
            roster.contains(&name.to_string()),
            "{name} should be in the roster the sidebar draws from"
        );
        // The active agent carries a `›`; it still names the agent.
        assert!(
            rows.iter().any(|row| row
                .split_whitespace()
                .any(|w| w.trim_start_matches('›') == name)),
            "the Agents tab should name `{name}`"
        );
    }
}

/// A roster of sixteen names is useless if the user cannot tell which one is
/// answering.
#[test]
fn the_agent_tab_marks_the_active_agent() {
    let mut app = TestApp::new();
    let active = app.active_agent();
    let rows = agent_tab_rows(&mut app);

    // `agent_tab_rows` pages through the column and concatenates every page,
    // so a row can arrive more than once. Compare the names the marker is
    // attached to, not the rendered rows. The marker is text, not only
    // colour: under NO_COLOR a colour-only mark says nothing.
    let mut marked: Vec<String> = rows
        .iter()
        .flat_map(|row| {
            row.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter_map(|word| word.strip_prefix('›').map(str::to_owned))
        .filter(|name| !name.is_empty())
        .collect();
    marked.sort();
    marked.dedup();
    assert_eq!(
        marked,
        vec![active],
        "exactly one agent should be marked active, and it should be the active one"
    );
}

/// The roster is names only.
///
/// Each entry used to carry a one-line description under it. That is reference
/// material for picking an agent by hand — sixteen names plus sixteen lines of
/// prose, in the pane that also has to show a delegation running right now.
/// The names stay because the roster answers "what exists"; the descriptions
/// went because nothing here acts on them.
#[test]
fn the_roster_is_names_only() {
    let mut app = TestApp::new();
    let rows = agent_tab_rows(&mut app);
    assert!(
        rows.iter()
            .any(|row| row.split_whitespace().any(|w| w == "fe")),
        "the roster should still name its agents: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("frontend")),
        "but not carry their descriptions: {rows:#?}"
    );
}

/// Names flow several to a row, so a sixteen-agent roster costs a handful of
/// rows rather than sixteen.
#[test]
fn the_roster_flows_names_across_rows() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(AGENT_TAB);
    let rows = app.side_column(W, H);
    let fe = rows
        .iter()
        .find(|row| {
            row.split_whitespace()
                .any(|w| w.trim_start_matches('›') == "fe")
        })
        .expect("the roster should name `fe`");
    let names_on_row = fe
        .split_whitespace()
        .filter(|w| !w.starts_with('│'))
        .count();
    assert!(
        names_on_row > 2,
        "fe should share its row with other agents: {fe:?}"
    );
    assert!(
        !fe.contains("accessibility") && !fe.contains("frontend"),
        "and carry no description: {fe:?}"
    );
}

#[test]
fn the_footer_names_the_active_agent() {
    let mut app = TestApp::new();
    let active = app.active_agent();
    let rows = screen(&mut app);
    // The footer is the last row inside the frame; searched from the bottom
    // because the roster in the sidebar names agents too.
    let footer = rows
        .iter()
        .rposition(|row| row.contains(&active))
        .expect("a row naming the active agent");
    assert!(
        footer + 3 >= rows.len(),
        "the agent name should sit in the footer, found at row {footer} of {}",
        rows.len()
    );
    // The legacy role label must not be what is shown.
    assert!(
        !rows[footer].contains("Orchestrator"),
        "the footer should show the agent, not the role: {:?}",
        rows[footer]
    );
}

#[test]
fn the_footer_follows_a_handover() {
    let mut app = TestApp::new();
    app.push_user("the login page is broken");
    let before = app.active_agent();
    app.switch_agent("fe", "the request is about the login page");
    let after = app.active_agent();
    assert_ne!(before, after, "a handover should change the active agent");

    let rows = screen(&mut app);
    assert!(
        rows.iter().rev().take(3).any(|row| row.contains("fe")),
        "the footer should name the agent that took over: {:?}",
        &rows[rows.len().saturating_sub(3)..]
    );
}

#[test]
fn a_handover_draws_a_marker_in_the_transcript() {
    let mut app = TestApp::new();
    app.push_user("the login page is broken");
    app.push_assistant("Let me look.");
    app.switch_agent("fe", "the request is about the login page");
    app.push_assistant("The stylesheet is missing.");

    let rows = screen(&mut app);
    let marker = rows
        .iter()
        .position(|row| row.contains("→ fe"))
        .expect("a handover marker naming the new agent");
    assert!(
        rows[marker].contains("the request is about the login page"),
        "the marker should carry the reason: {:?}",
        rows[marker]
    );
    assert!(
        rows[marker].contains("orchestrator"),
        "the marker should name who handed over: {:?}",
        rows[marker]
    );
}

/// The marker is punctuation between two agents' work, so it has to sit
/// between them — above the first thing the new agent says, below the last
/// thing the old one said.
#[test]
fn the_marker_sits_between_the_two_agents_work() {
    let mut app = TestApp::new();
    app.push_assistant("BEFORE-THE-SWITCH");
    app.switch_agent("fe", "handing over");
    app.push_assistant("AFTER-THE-SWITCH");

    let rows = screen(&mut app);
    let find = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle} should be on screen"))
    };
    let before = find("BEFORE-THE-SWITCH");
    let marker = find("→ fe");
    let after = find("AFTER-THE-SWITCH");
    assert!(
        before < marker && marker < after,
        "marker at {marker} should fall between {before} and {after}"
    );
}

#[test]
fn a_session_without_switches_draws_no_markers() {
    let mut app = TestApp::new();
    app.push_user("hello");
    app.push_assistant("Hi. Nothing here changed hands.");

    let rows = screen(&mut app);
    for row in &rows {
        assert!(
            !row.contains(" → "),
            "an unswitched session should carry no handover rule: {row:?}"
        );
    }
}

/// A switch recorded against `at_turn` has to land in the right place once
/// the transcript is rebuilt from a file — the turn index alone does not say
/// where, because one turn becomes several blocks.
#[test]
fn a_resumed_session_places_its_markers_by_turn() {
    let mut app = TestApp::new();
    app.resume_with_switches(
        &[
            ("user", "TURN-ZERO-QUESTION"),
            ("assistant", "TURN-ONE-ANSWER"),
            ("user", "TURN-TWO-QUESTION"),
            ("assistant", "TURN-THREE-ANSWER"),
        ],
        &[("router", "be", "this is an API problem", 2)],
    )
    .expect("the fixture session should resume");

    let rows = screen(&mut app);
    let find = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle} should be on screen, got {rows:?}"))
    };
    let one = find("TURN-ONE-ANSWER");
    let marker = find("→ be");
    let two = find("TURN-TWO-QUESTION");
    assert!(
        one < marker && marker < two,
        "a switch at turn 2 belongs between turn 1 and turn 2, got {one} / {marker} / {two}"
    );
    assert_eq!(
        app.active_agent(),
        "be",
        "resuming should adopt the session's agent"
    );
}

/// Markers are rendered inside the block cache, so they are the thing most
/// likely to freeze on screen if the cache key forgets them.
#[test]
fn a_marker_does_not_freeze_the_render_cache() {
    let mut app = TestApp::new();
    app.push_assistant("first");
    let before = screen(&mut app);
    assert!(
        !before.iter().any(|row| row.contains("→ fe")),
        "no marker yet"
    );

    // Switch WITHOUT pushing a block. The marker lands on a block that has
    // already been rendered and whose text has not changed, so nothing but
    // the marker itself can invalidate its cache entry — which is exactly the
    // case a key that forgot the marker would serve stale.
    app.switch_agent("fe", "handing over");
    let warm = screen(&mut app);
    assert!(
        warm.iter().any(|row| row.contains("→ fe")),
        "the marker must appear rather than the cached frame being served again"
    );

    app.clear_render_cache();
    let cold = screen(&mut app);
    assert_eq!(
        warm, cold,
        "a frame carrying a marker must match what a cold render produces"
    );

    // And the same the other way: a second handover on the same block must
    // replace what is shown, not sit under the first one's cached rows.
    app.switch_agent("be", "and again");
    let again = screen(&mut app);
    assert!(
        again.iter().any(|row| row.contains("→ be")),
        "a second handover on an unchanged block must re-render it"
    );
}

/// The marker must be counted by the pass that sizes the transcript, or the
/// scrollbar and the scroll offset disagree with what is drawn.
#[test]
fn a_marker_adds_rows_to_the_transcript() {
    let mut app = TestApp::new();
    for i in 0..40 {
        app.push_assistant(&format!("line {i}"));
    }
    let _ = screen(&mut app);
    let before = app.max_scroll();

    app.switch_agent("fe", "handing over");
    app.push_assistant("after");
    let _ = screen(&mut app);
    assert!(
        app.max_scroll() > before,
        "the marker's rows should extend the transcript: {before} -> {}",
        app.max_scroll()
    );
}
