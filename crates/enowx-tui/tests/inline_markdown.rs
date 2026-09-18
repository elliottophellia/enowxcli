//! Inline markdown is parsed by hand, so the cases that matter are the ones
//! where a coding agent's prose collides with markdown's delimiters: shell
//! globs, arithmetic, identifiers with underscores, and code spans that
//! themselves contain backticks.

use enowx_tui::preview_markdown;

/// Render at a generous width and strip styling, leaving the visible text.
fn text(md: &str) -> String {
    preview_markdown(md, 200)
        .join("\n")
        .replace(|c: char| c == '\u{1b}', "")
}

fn plain(md: &str) -> String {
    let re_stripped: String = {
        let mut out = String::new();
        let mut in_escape = false;
        for c in text(md).chars() {
            if c == '[' && in_escape {
                continue;
            }
            if in_escape {
                if c == 'm' {
                    in_escape = false;
                }
                continue;
            }
            if c == '\u{1b}' {
                in_escape = true;
                continue;
            }
            out.push(c);
        }
        out
    };
    re_stripped
}

#[test]
fn arithmetic_keeps_its_asterisks() {
    let got = plain("Math: 2 * 3 * 4 = 24");
    assert!(
        got.contains("2 * 3 * 4 = 24"),
        "asterisks used as multiplication must survive, got {got:?}"
    );
}

#[test]
fn a_glob_pattern_is_not_emphasis() {
    let got = plain("Run rm *.log and *.tmp to clean up");
    assert!(
        got.contains("*.log") && got.contains("*.tmp"),
        "glob stars must survive, got {got:?}"
    );
}

#[test]
fn escaped_delimiters_render_literally() {
    let got = plain(r"Escaped: \*not italic\* and \`not code\`");
    assert!(
        got.contains("*not italic*"),
        "an escaped asterisk must show as a literal star, got {got:?}"
    );
    assert!(
        !got.contains('\\'),
        "the backslash itself must not be shown, got {got:?}"
    );
}

#[test]
fn double_backticks_can_contain_a_backtick() {
    let got = plain("Use ``a ` b`` here");
    assert!(
        got.contains("a ` b"),
        "a double-backtick span must keep the inner backtick, got {got:?}"
    );
}

#[test]
fn strikethrough_text_is_kept() {
    let got = plain("This is ~~removed~~ now");
    assert!(
        got.contains("removed") && !got.contains("~~"),
        "strikethrough must render its text without the tildes, got {got:?}"
    );
}

#[test]
fn identifiers_with_underscores_are_untouched() {
    let got = plain("Call some_var_name and other_var_here now");
    assert!(
        got.contains("some_var_name") && got.contains("other_var_here"),
        "underscores inside identifiers must not be treated as emphasis, got {got:?}"
    );
}

#[test]
fn real_emphasis_still_works() {
    let got = plain("This is **bold** and *italic* and `code`.");
    for word in ["bold", "italic", "code"] {
        assert!(got.contains(word), "{word} should still render");
    }
    assert!(
        !got.contains("**") && !got.contains('`'),
        "delimiters themselves must not be shown, got {got:?}"
    );
}

#[test]
fn an_unterminated_delimiter_is_shown_literally() {
    let got = plain("A lone * star and a lone ` tick");
    assert!(
        got.contains('*') && got.contains('`'),
        "unmatched delimiters must not swallow the rest of the line, got {got:?}"
    );
}

#[test]
fn a_link_shows_its_label_not_its_url() {
    let got = plain("See [the docs](https://example.com/page) for more");
    assert!(got.contains("the docs"), "the label must render");
    assert!(
        !got.contains("example.com"),
        "the URL must stay out of the prose, got {got:?}"
    );
}
