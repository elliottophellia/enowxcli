//! An edit by line anchors (`edit_lines`, or `edit` called with anchors)
//! shows as a diff, as `edit` does, not as the tool's raw text.

use enowx_tui::testing::TestApp;

const RESULT: &str = "Updated index.html: 1 edits (- removed; + added, with the new anchors)
    5#a1c:<head>
-         <title>Old</title>
+   6#22b:<title>Observatory</title>
+   7#eb0:<script id=\"theme-init\"></script>
    8#d7a:</head>
  …";

fn shows(name: &str, args: &str) -> String {
    let mut app = TestApp::in_conversation();
    app.push_user("fix the theme start");
    app.push_tool("t1", name, args, RESULT);
    app.render_to_text(120, 40).join("\n")
}

#[test]
fn an_edit_by_anchor_is_drawn_as_a_diff() {
    let screen = shows(
        "edit_lines",
        r#"{"path":"index.html","edits":[{"op":"replace","from":"6#00a","text":"x"}]}"#,
    );
    assert!(screen.contains("+2 -1 · 1 edit"), "{screen}");
    assert!(screen.contains("Observatory"), "{screen}");
    assert!(
        !screen.contains("6#22b"),
        "no anchors in the diff: {screen}"
    );
}

/// `edit` called with anchors runs as `edit_lines`, and shows as one.
#[test]
fn an_edit_called_with_anchors_shows_the_same() {
    let screen = shows(
        "edit",
        r#"{"path":"index.html","edits":[{"op":"replace","from":"6#00a","text":"x"}]}"#,
    );
    assert!(screen.contains("+2 -1 · 1 edit"), "{screen}");
    assert!(!screen.contains("6#22b"), "{screen}");
}

/// A result from before the diff format stays readable as text.
#[test]
fn an_older_result_stays_as_text() {
    let mut app = TestApp::in_conversation();
    app.push_user("x");
    app.push_tool(
        "t1",
        "edit_lines",
        r#"{"path":"a.txt","edits":[{"op":"replace","from":"2#abc","text":"TWO"}]}"#,
        "Updated a.txt: 1 edits\n   1#aaa:one\n   2#bbb:TWO\n  …",
    );
    let screen = app.render_to_text(120, 40).join("\n");
    assert!(screen.contains("1 edit"), "{screen}");
}
